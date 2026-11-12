//! IndexedDB 题库与复盘状态存储。
//!
//! 分类按 generation 分键；活动快照只由 `active_generation` 指向。
//! 本模块只处理 IndexedDB；ZIP 备份由 `backup` 解析和生成。

use async_trait::async_trait;
use indexed_db_futures::database::Database;
use indexed_db_futures::prelude::*;
use indexed_db_futures::transaction::TransactionMode;
use js_sys::{Array, Reflect};
use mc_core::ports::{CorpusSnapshot, CorpusStore, ReviewStore};
use mc_core::{Category, Corpus, ReviewState};
use wasm_bindgen::{JsCast, JsValue};

pub const DB_NAME: &str = "mistake-compass";
const META: &str = "meta";
const CATEGORIES: &str = "categories";
const REVIEW: &str = "review";

/// IndexedDB 存储
pub struct IndexedDbStore {
    db: Database,
}

impl IndexedDbStore {
    /// 打开默认数据库。
    ///
    /// # Errors
    ///
    /// 浏览器拒绝打开数据库或创建对象仓库失败时返回错误。
    pub async fn open() -> Result<IndexedDbStore, String> {
        Self::open_named(DB_NAME).await
    }

    /// 打开指定名称的数据库；测试用独立数据库隔离数据。
    ///
    /// # Errors
    ///
    /// 浏览器拒绝打开数据库或创建对象仓库失败时返回错误。
    pub async fn open_named(name: &str) -> Result<IndexedDbStore, String> {
        let db = Database::open(name)
            .with_version(1u8)
            .with_on_upgrade_needed(|_event, db| {
                if !db.object_store_names().any(|name| name == META) {
                    db.create_object_store(META).build()?;
                }
                if !db.object_store_names().any(|name| name == CATEGORIES) {
                    db.create_object_store(CATEGORIES).build()?;
                }
                if !db.object_store_names().any(|name| name == REVIEW) {
                    db.create_object_store(REVIEW).build()?;
                }
                Ok(())
            })
            .await
            .map_err(|error| format!("打开 IndexedDB 失败：{error}"))?;
        Ok(IndexedDbStore { db })
    }

    async fn read_meta_string(&self, key: &str) -> Result<Option<String>, String> {
        let tx = self
            .db
            .transaction(META)
            .build()
            .map_err(|error| format!("打开 meta 事务失败：{error}"))?;
        let store = tx
            .object_store(META)
            .map_err(|error| format!("打开 meta 仓库失败：{error}"))?;
        let value: Option<JsValue> = store
            .get(key)
            .await
            .map_err(|error| format!("读取 {key} 失败：{error}"))?;
        Ok(value.and_then(|value| value.as_string()))
    }

    async fn put_meta_string(&self, key: &str, value: &str) -> Result<(), String> {
        let tx = self
            .db
            .transaction(META)
            .with_mode(TransactionMode::Readwrite)
            .build()
            .map_err(|error| format!("打开 meta 写事务失败：{error}"))?;
        let store = tx
            .object_store(META)
            .map_err(|error| format!("打开 meta 仓库失败：{error}"))?;
        store
            .put(JsValue::from_str(value))
            .with_key(key)
            .await
            .map_err(|error| format!("写入 {key} 失败：{error}"))?;
        tx.commit()
            .await
            .map_err(|error| format!("提交 meta 写事务失败：{error}"))
    }

    async fn read_categories(&self, generation: &str) -> Result<Vec<Category>, String> {
        let tx = self
            .db
            .transaction(CATEGORIES)
            .build()
            .map_err(|error| format!("打开 categories 事务失败：{error}"))?;
        let store = tx
            .object_store(CATEGORIES)
            .map_err(|error| format!("打开 categories 仓库失败：{error}"))?;
        let records: Vec<JsValue> = store
            .get_all()
            .await
            .map_err(|error| format!("读取 categories 失败：{error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("读取 categories 失败：{error}"))?;
        let mut categories = Vec::new();
        for record in records {
            let record_generation = string_field(&record, "generation")?;
            if record_generation != generation {
                continue;
            }
            let path = string_field(&record, "path")?;
            let yaml = string_field(&record, "yaml")?;
            categories.push(
                mc_core::parse_category(&path, &yaml)
                    .map_err(|error| format!("分类 {path} 解析失败：{error}"))?,
            );
        }
        Ok(categories)
    }

    /// 导出当前活动题库和复盘状态为 ZIP。
    ///
    /// # Errors
    ///
    /// 没有活动题库、读取状态或写 ZIP 失败时返回错误。
    pub async fn export_archive(&self) -> Result<Vec<u8>, String> {
        let active = self
            .active()
            .await?
            .ok_or_else(|| "当前没有题库，无法导出".to_string())?;
        let review = self.load().await?;
        let exported_at = js_sys::Date::new_0()
            .to_iso_string()
            .as_string()
            .unwrap_or_default();
        super::backup::build_archive(active.corpus.categories(), &review, &exported_at)
    }

    /// 导入 ZIP，并在同一事务内切换到新 generation。
    ///
    /// 导入先完整解析和校验备份，再写入分类、复盘状态和活动指针。
    ///
    /// # Errors
    ///
    /// 备份校验、写库或事务提交失败时返回错误。
    pub async fn import_archive(
        &self,
        bytes: &[u8],
    ) -> Result<super::backup::ImportSnapshot, String> {
        let snapshot = super::backup::parse_archive(bytes)?;
        self.import_snapshot(snapshot.categories.clone(), &snapshot.review)
            .await?;
        Ok(snapshot)
    }

    async fn import_snapshot(
        &self,
        categories: Vec<Category>,
        review: &ReviewState,
    ) -> Result<(), String> {
        let generation = format!("import-{}", js_sys::Date::now() as u64);
        let tx = self
            .db
            .transaction([META, CATEGORIES, REVIEW])
            .with_mode(TransactionMode::Readwrite)
            .build()
            .map_err(|error| format!("打开导入事务失败：{error}"))?;
        let meta = tx
            .object_store(META)
            .map_err(|error| format!("打开 meta 仓库失败：{error}"))?;
        let store = tx
            .object_store(CATEGORIES)
            .map_err(|error| format!("打开 categories 仓库失败：{error}"))?;
        let review_store = tx
            .object_store(REVIEW)
            .map_err(|error| format!("打开 review 仓库失败：{error}"))?;
        for category in &categories {
            let yaml = sync::emit::render_category(category);
            let record = js_sys::Object::new();
            Reflect::set(
                &record,
                &JsValue::from_str("generation"),
                &JsValue::from_str(&generation),
            )
            .map_err(|_| "写入 generation 字段失败".to_string())?;
            Reflect::set(
                &record,
                &JsValue::from_str("path"),
                &JsValue::from_str(&category.path),
            )
            .map_err(|_| "写入 path 字段失败".to_string())?;
            Reflect::set(
                &record,
                &JsValue::from_str("question_count"),
                &JsValue::from_f64(category.questions.len() as f64),
            )
            .map_err(|_| "写入 question_count 字段失败".to_string())?;
            Reflect::set(
                &record,
                &JsValue::from_str("yaml"),
                &JsValue::from_str(&yaml),
            )
            .map_err(|_| "写入 yaml 字段失败".to_string())?;
            let key = Array::new();
            key.push(&JsValue::from_str(&generation));
            key.push(&JsValue::from_str(&category.path));
            store
                .put(record)
                .with_key(key)
                .await
                .map_err(|error| format!("写入分类 {} 失败：{error}", category.path))?;
        }
        review_store
            .put(JsValue::from_str(&review.to_yaml()?))
            .with_key("state")
            .await
            .map_err(|error| format!("写入复盘状态失败：{error}"))?;
        meta.put(JsValue::from_f64(1.0))
            .with_key("format_version")
            .await
            .map_err(|error| format!("写入 format_version 失败：{error}"))?;
        meta.put(JsValue::from_str(&generation))
            .with_key("active_generation")
            .await
            .map_err(|error| format!("写入 active_generation 失败：{error}"))?;
        tx.commit()
            .await
            .map_err(|error| format!("提交导入事务失败：{error}"))
    }
}

#[async_trait(?Send)]
impl CorpusStore for IndexedDbStore {
    async fn active(&self) -> Result<Option<CorpusSnapshot>, String> {
        let Some(generation) = self.read_meta_string("active_generation").await? else {
            return Ok(None);
        };
        let categories = self.read_categories(&generation).await?;
        let corpus = Corpus::new(categories)?;
        Ok(Some(CorpusSnapshot { generation, corpus }))
    }

    async fn stage(&self, generation: &str, categories: Vec<Category>) -> Result<(), String> {
        let tx = self
            .db
            .transaction([META, CATEGORIES])
            .with_mode(TransactionMode::Readwrite)
            .build()
            .map_err(|error| format!("打开题库写事务失败：{error}"))?;
        let meta = tx
            .object_store(META)
            .map_err(|error| format!("打开 meta 仓库失败：{error}"))?;
        let store = tx
            .object_store(CATEGORIES)
            .map_err(|error| format!("打开 categories 仓库失败：{error}"))?;
        for category in &categories {
            let yaml = sync::emit::render_category(category);
            let record = js_sys::Object::new();
            Reflect::set(
                &record,
                &JsValue::from_str("generation"),
                &JsValue::from_str(generation),
            )
            .map_err(|_| "写入 generation 字段失败".to_string())?;
            Reflect::set(
                &record,
                &JsValue::from_str("path"),
                &JsValue::from_str(&category.path),
            )
            .map_err(|_| "写入 path 字段失败".to_string())?;
            Reflect::set(
                &record,
                &JsValue::from_str("question_count"),
                &JsValue::from_f64(category.questions.len() as f64),
            )
            .map_err(|_| "写入 question_count 字段失败".to_string())?;
            Reflect::set(
                &record,
                &JsValue::from_str("yaml"),
                &JsValue::from_str(&yaml),
            )
            .map_err(|_| "写入 yaml 字段失败".to_string())?;
            let key = Array::new();
            key.push(&JsValue::from_str(generation));
            key.push(&JsValue::from_str(&category.path));
            store
                .put(record)
                .with_key(key)
                .await
                .map_err(|error| format!("写入分类 {} 失败：{error}", category.path))?;
        }
        meta.put(JsValue::from_f64(1.0))
            .with_key("format_version")
            .await
            .map_err(|error| format!("写入 format_version 失败：{error}"))?;
        tx.commit()
            .await
            .map_err(|error| format!("提交题库写事务失败：{error}"))
    }

    async fn activate(&self, generation: &str) -> Result<(), String> {
        self.put_meta_string("active_generation", generation).await
    }

    async fn discard(&self, generation: &str) -> Result<(), String> {
        let tx = self
            .db
            .transaction(CATEGORIES)
            .with_mode(TransactionMode::Readwrite)
            .build()
            .map_err(|error| format!("打开清理事务失败：{error}"))?;
        let store = tx
            .object_store(CATEGORIES)
            .map_err(|error| format!("打开 categories 仓库失败：{error}"))?;
        let keys: Vec<JsValue> = store
            .get_all_keys()
            .await
            .map_err(|error| format!("读取分类键失败：{error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("读取分类键失败：{error}"))?;
        for key in keys {
            if key
                .dyn_ref::<Array>()
                .and_then(|array| array.get(0).as_string())
                .as_deref()
                == Some(generation)
            {
                store
                    .delete(key)
                    .await
                    .map_err(|error| format!("删除分类键失败：{error}"))?;
            }
        }
        tx.commit()
            .await
            .map_err(|error| format!("提交清理事务失败：{error}"))
    }
}

#[async_trait(?Send)]
impl ReviewStore for IndexedDbStore {
    async fn load(&self) -> Result<ReviewState, String> {
        let tx = self
            .db
            .transaction(REVIEW)
            .build()
            .map_err(|error| format!("打开 review 事务失败：{error}"))?;
        let store = tx
            .object_store(REVIEW)
            .map_err(|error| format!("打开 review 仓库失败：{error}"))?;
        let value: Option<JsValue> = store
            .get("state")
            .await
            .map_err(|error| format!("读取复盘状态失败：{error}"))?;
        match value.and_then(|value| value.as_string()) {
            Some(text) => ReviewState::from_yaml(&text),
            None => Ok(ReviewState::default()),
        }
    }

    async fn save(&self, state: &ReviewState) -> Result<(), String> {
        let text = state.to_yaml()?;
        let tx = self
            .db
            .transaction(REVIEW)
            .with_mode(TransactionMode::Readwrite)
            .build()
            .map_err(|error| format!("打开 review 写事务失败：{error}"))?;
        let store = tx
            .object_store(REVIEW)
            .map_err(|error| format!("打开 review 仓库失败：{error}"))?;
        store
            .put(JsValue::from_str(&text))
            .with_key("state")
            .await
            .map_err(|error| format!("写入复盘状态失败：{error}"))?;
        tx.commit()
            .await
            .map_err(|error| format!("提交复盘状态失败：{error}"))
    }
}

fn string_field(value: &JsValue, field: &str) -> Result<String, String> {
    Reflect::get(value, &JsValue::from_str(field))
        .map_err(|_| format!("记录缺字段 {field}"))?
        .as_string()
        .ok_or_else(|| format!("记录字段 {field} 不是字符串"))
}
