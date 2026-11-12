//! 错题罗盘领域模型、纯逻辑与端口契约。
//!
//! 持久化、网络和时钟通过 `ports` 注入，本 crate 不依赖具体平台。

pub mod corpus;
pub mod ports;
pub mod question;
pub mod review;
pub mod score;
pub mod testing;

pub use corpus::{DirNode, FileNode, Level, build_level, build_tree, parse_category};
pub use question::{Category, Corpus, FIELDS, Number, Question, QuestionType};
pub use review::{Document, Entry, Record, ReviewState, State};
