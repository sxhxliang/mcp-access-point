# MCP接入网关  

MCP接入网关是一款轻量级的协议转换网关工具，专门用于在传统HTTP服务与MCP（模型上下文协议）客户端之间建立通信桥梁。它使得MCP客户端无需任何服务端接口改造，即可直接与现有HTTP服务进行交互。  

## 简介
本项目基于Pingora——一个超高性能的网关代理库，能够支撑超大规模的请求代理服务。Pingora已被用于构建支撑Cloudflare平台核心流量处理的服务体系，多年来持续为互联网提供每秒超过4000万次请求的服务能力，目前已成为Cloudflare平台上处理相当大比例流量的技术基石。

## Http to SSE
此模式允许 Cursor Desktop 等客户端通过 SSE 与远程Http服务器通信，即使它本身不受支持SSE协议。

```mermaid
graph LR
    A["Cursor Desktop"] <--> |sse| B["MCP Access Point"]
    B <--> |http| C["Existing Http Server"]

    style A fill:#ffe6f9,stroke:#333,color:black,stroke-width:2px
    style B fill:#e6e6ff,stroke:#333,color:black,stroke-width:2px
    style C fill:#e6ffe6,stroke:#333,color:black,stroke-width:2px
```

## 快速开始  

### 安装方式  
```bash
# 从源码安装
git clone https://github.com/sxhxliang/mcp-access-point.git
cd mcp-access-point
# 传入openapi.json文件路径、mcp端口号、上游服务地址
cargo run -- -f openapi_for_demo.json -p 8080 -u localhost:8090
# 也可以使用远程服务器的openapi.json，比如petstore.swagger.io
cargo run -- -f https://petstore.swagger.io/v2/swagger.json -p 8080 -u localhost:8090
# 使用inspector调试，先启动服务
npx @modelcontextprotocol/inspector@0.8.1 node build/index.js
# 访问 http://127.0.0.1:6274/
# 选择 see 填入0.0.0.0:8080/sse, 点击connect就可以连接上服务啦
```
 

### 参数详解：  
1. **`-f openapi_for_demo.json`**  
   - `-f`（或 `--file`）指定 OpenAPI 规范文件的路径（`openapi_for_demo.json`）。  
   - 该文件定义了 MCP（Model Context Protocol）接入点要代理转换的 API。  

2. **`-p 8080`**  
   - `-p`（或 `--port`）设置 MCP 接入点的监听端口（`8080`），用于接收客户端请求。  

3. **`-u localhost:8090`**  
   - `-u`（或 `--upstream`）指定上游服务的地址（`localhost:8090`）。  
   - MCP 接入点会在处理请求后，将其转发到该地址对应的后端服务。  



## 核心特性  

- **双向协议转换**：实现HTTP与MCP协议的双向无缝转换  
- **零侵入式接入**：完全兼容现有HTTP服务，无需任何改造  
- **客户端赋能**：让MCP生态客户端能够直接调用标准HTTP服务  
- **轻量级代理**：极简架构设计，协议转换高效透明  

## 使用Docker运行

### 构建Docker镜像（可选，如果你想本地构建）
```bash
# 克隆仓库
git clone https://github.com/sxhxliang/mcp-access-point.git
cd mcp-access-point

# 构建Docker镜像
docker build -t kames2025/mcp-access-point:latest .
```

### 拉取并运行Docker容器
```bash
# 使用环境变量配置（上游服务在宿主机上运行）
# 注意：将 /path/to/your/openapi.json 替换为你本地 OpenAPI 文件的实际路径
# 注意：upstream 地址使用了 host.docker.internal 来指向宿主机，如果无效请尝试宿主机的局域网IP
docker run -d --name mcp-access-point --rm \
  -p 8080:8080 \
  -e port=8080 \
  -e upstream=host.docker.internal:8090 \
  -e openapi_json=/app/config/openapi.json \
  -v /path/to/your/openapi.json:/app/config/openapi.json \
  kames2025/mcp-access-point:latest

# 或者直接指定openapi_json环境变量
docker run -d --name mcp-access-point --rm \
  -p 8080:8080 \
  -e port=8080 \
  -e upstream=host.docker.internal:8090 \
  -e openapi_json=/app/config/openapi.json \
  -v /path/to/your/openapi.json:/app/config/openapi.json \
  kames2025/mcp-access-point:latest
```

### 环境变量说明
- `port`: MCP接入网关监听端口，默认为8080
- `upstream`: 上游服务地址，默认为localhost:8090
- `openapi_json`: OpenAPI规范文件路径，默认为/app/config/openapi.json

## 典型应用场景  

- **渐进式架构迁移**：帮助HTTP服务逐步过渡到MCP架构体系  
- **混合架构支持**：在MCP生态中复用现有HTTP基础设施  
- **协议兼容方案**：构建同时支持双协议体系的混合系统  

**典型案例**：  
当采用MCP协议的AI客户端需要对接企业遗留的HTTP微服务时，MCP接入网关可作为中间层，实现协议的无缝转换。



