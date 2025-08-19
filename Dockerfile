# 构建阶段
FROM rust:1.75 as builder

WORKDIR /app

# 复制 Cargo 文件
COPY Cargo.toml Cargo.lock ./

# 创建一个虚拟的 main.rs 来预编译依赖
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm src/main.rs

# 复制源代码
COPY src ./src

# 构建应用
RUN cargo build --release

# 运行阶段 - 使用 scratch
FROM scratch

# 复制编译好的二进制文件
COPY --from=builder /app/target/release/m2proxy /m2proxy

# 暴露端口
EXPOSE 1234

# 设置入口点
ENTRYPOINT ["/m2proxy"]
