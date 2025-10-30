FROM rust:1.85-bookworm AS builder

WORKDIR /app

# 复制项目文件
COPY . .
RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl-dev \
    pkg-config \
    cmake \
    libclang-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# 构建项目
RUN cargo build --release

# 使用精简的基础镜像
FROM debian:bookworm-slim

# 安装必要的依赖
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    libgcc-s1 \
    tzdata \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# 复制编译好的可执行文件
COPY --from=builder /app/target/release/access-point /app/access-point
# 创建配置目录
RUN mkdir -p /app/config

# 创建启动脚本
COPY docker-entrypoint.sh /app/
RUN chmod +x /app/docker-entrypoint.sh

EXPOSE 8080

ENTRYPOINT ["/app/docker-entrypoint.sh"]
