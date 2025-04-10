FROM rust:latest as builder

WORKDIR /app

# 复制项目文件
COPY . .

# 构建项目
RUN cargo build --release

# 使用精简的基础镜像
FROM debian:bookworm-slim

# 安装必要的依赖
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# 复制编译好的可执行文件
COPY --from=builder /app/target/release/mcp-access-point /app/mcp-access-point
# 创建配置目录
RUN mkdir -p /app/config

# 创建启动脚本
COPY docker-entrypoint.sh /app/
RUN chmod +x /app/docker-entrypoint.sh

EXPOSE 8080

ENTRYPOINT ["/app/docker-entrypoint.sh"]
