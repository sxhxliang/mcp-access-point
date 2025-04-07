# MCP接入网关  

MCP接入网关是一款轻量级的协议转换网关工具，专门用于在传统HTTP服务与MCP（模型上下文协议）客户端之间建立通信桥梁。它使得MCP客户端无需任何服务端接口改造，即可直接与现有HTTP服务进行交互。  

## 快速开始  

### 安装方式  
```bash
# 从源码安装
git clone https://github.com/sxhxliang/mcp-access-point.git
cd mcp-access-point
# 传入openapi.json文件路径、mcp端口号、上游服务地址
cargo run -- -f openapi_for_demo.json -p 8080 -u localhost:8090
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

## 典型应用场景  

- **渐进式架构迁移**：帮助HTTP服务逐步过渡到MCP架构体系  
- **混合架构支持**：在MCP生态中复用现有HTTP基础设施  
- **协议兼容方案**：构建同时支持双协议体系的混合系统  

**典型案例**：  
当采用MCP协议的AI客户端需要对接企业遗留的HTTP微服务时，MCP接入网关可作为中间层，实现协议的无缝转换。



