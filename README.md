# mistake-compass

把粉笔错题抓到本地、按分类复盘的自用工具。

## 下载

到 [Releases](https://github.com/p2mm2p/mistake-compass/releases) 下载对应平台的压缩包，解压到任意目录。

| 平台 | 启动脚本 |
| --- | --- |
| Windows | `1.登陆粉笔.bat`、`2.下载错题.bat`、`3.开始做题.bat` |
| Linux | `1.登陆粉笔.sh`、`2.下载错题.sh`、`3.开始做题.sh` |
| macOS | `1.登陆粉笔.command`、`2.下载错题.command`、`3.开始做题.command` |

macOS 的 `.command` 可在 Finder 双击运行。Linux 的 `.sh` 在终端运行，例如 `./1.登陆粉笔.sh`。

## 使用

1. 先登录：运行 1 号脚本，在打开的浏览器里完成登录。
2. 再下载错题：运行 2 号脚本，等抓取结束。
3. 最后做题：运行 3 号脚本，在浏览器打开提示的本地地址，默认 http://127.0.0.1:8787。

登录需要本机已安装 Edge 或 Chrome。macOS 首次运行被系统拦截时，右键脚本选择“打开”。

## 数据

运行数据保存在解压目录：

- `auth/`：登录态
- `data/`：错题
- `review/`：复盘状态

离线使用时，题目里的图片链接会加载失败，属预期行为。

## 从源码构建

需要 Rust 工具链。在仓库根目录运行：

```sh
cargo build --release
```

产物是 `target/release/mistake-compass`，Windows 下是 `mistake-compass.exe`。
