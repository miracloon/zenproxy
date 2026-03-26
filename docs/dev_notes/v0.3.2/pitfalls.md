# v0.3.2 Pitfalls — Rust musl 交叉编译踩坑记录

> 在 GitHub Actions 中为 arm64 构建 Rust musl 静态二进制时遇到的问题。

## 背景

原始方案使用 QEMU 模拟 arm64 环境编译，构建时间约 60 分钟。尝试改为原生交叉编译以提速。

## 踩坑过程

### 1. Alpine + rust-lld（失败）

直接在 `rust:alpine` 镜像中用 `rustup target add aarch64-unknown-linux-musl` + `rust-lld` 做交叉编译。

**报错**：`linking with rust-lld failed: unable to find library -lgcc_s / -lc`

**原因**：Alpine 只安装了本机架构的 musl-dev，缺少 aarch64 的 musl 库文件。rust-lld 找不到目标架构的 libc。

### 2. Debian + gcc-aarch64-linux-gnu（失败）

换用 `rust:slim-bookworm`（Debian 基础），安装 `gcc-aarch64-linux-gnu` 交叉编译器。

**报错**：`exit code: 101`，链接阶段失败。

**原因**：`gcc-aarch64-linux-gnu` 提供的是 **glibc** 交叉工具链，而 Rust target `aarch64-unknown-linux-musl` 需要 **musl** 工具链。两者不兼容。

### 3. tonistiigi/xx + gcc linker（失败）

使用 Docker 官方交叉编译工具包 `tonistiigi/xx`，通过 `xx-apk add gcc` 安装目标架构编译器，配置 `aarch64-alpine-linux-musl-gcc` 作为链接器。

**报错**：`cc-rs: failed to find tool "aarch64-alpine-linux-musl-gcc"`

**原因**：`xx-apk add gcc` 安装的 gcc 并不以 `aarch64-alpine-linux-musl-gcc` 命名。xx 工具链设计上以 **clang** 为核心，不提供命名直觉一致的 gcc 前缀。

### 4. tonistiigi/xx + xx-clang linker（失败）

改用 `xx-clang` 作为 Rust 链接器（通过 `.cargo/config.toml` 配置），解决了 Rust 自身的链接问题。

**报错**：`cc-rs: failed to find tool "aarch64-linux-musl-gcc"`

**原因**：`rusqlite` 依赖通过 `cc` crate 编译内嵌 SQLite C 代码。`cc` crate 独立查找 C 编译器，不受 Rust 链接器配置影响。它按照惯例查找 `{target-triple}-gcc`，找不到就报错。

### 5. 设置 CC 环境变量（成功）

在 Dockerfile 中设置：
```dockerfile
ENV CC_aarch64_unknown_linux_musl=/usr/bin/xx-clang
ENV CFLAGS_aarch64_unknown_linux_musl="--target=aarch64-alpine-linux-musl --sysroot=/aarch64-alpine-linux-musl"
```

`cc` crate 识别 `CC_{target}` 环境变量，直接使用 `xx-clang` 编译 C 代码，并通过 `CFLAGS` 传入正确的 target 和 sysroot。

### 6. Cargo 缓存导致空二进制（成功修复）

交叉编译本身成功后，产出的二进制文件运行即退出（exit code 0，无任何输出）。

**原因**：Dockerfile 中为了缓存依赖编译，先写入 `echo 'fn main() {}' > src/main.rs` 编译依赖，再 `COPY src /build/src` 覆盖真实源码。但 Docker layer 的文件时间戳可能导致 Cargo 认为 `src/main.rs` 未变更，跳过重编译，产出空的 `fn main() {}` 二进制。

**修复**：在真实编译前加 `touch src/main.rs` 强制刷新时间戳。

## 最终方案

```dockerfile
FROM --platform=$BUILDPLATFORM tonistiigi/xx AS xx
FROM --platform=$BUILDPLATFORM rust:1.86-alpine AS builder-rust
COPY --from=xx / /

RUN apk add --no-cache clang lld musl-dev
RUN xx-apk add --no-cache musl-dev

ENV CC_aarch64_unknown_linux_musl=/usr/bin/xx-clang
ENV CFLAGS_aarch64_unknown_linux_musl="--target=aarch64-alpine-linux-musl --sysroot=/aarch64-alpine-linux-musl"

# .cargo/config.toml: [target.aarch64-unknown-linux-musl] linker = "/usr/bin/xx-clang"
```

关键配置点：
1. **Rust 链接器**：`.cargo/config.toml` 中指定 `xx-clang`
2. **C 编译器**：`CC_{target}` 环境变量指向 `xx-clang`
3. **C 编译标志**：`CFLAGS_{target}` 传入 `--target` 和 `--sysroot`
4. **缓存失效**：编译前 `touch src/main.rs`
