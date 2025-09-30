# MCP Access Point

`MCP Access Point`是一款轻量级的协议转换网关工具，专门用于在传统`HTTP`服务与`MCP`（模型上下文协议）客户端之间建立通信桥梁。它使得MCP客户端无需任何服务端接口改造，即可直接与现有HTTP服务进行交互。  
<p align="center">
  <a href="./README.md"><img alt="README in English" src="https://img.shields.io/badge/English-4578DA"></a>
  <a href="./README_CN.md"><img alt="简体中文版" src="https://img.shields.io/badge/简体中文-F40002"></a>
  <a href="https://deepwiki.com/sxhxliang/mcp-access-point"><img src="https://deepwiki.com/badge.svg" alt="Ask DeepWiki"></a>
  <a href="https://zread.ai/sxhxliang/mcp-access-point"><img alt="中文文档" src="https://img.shields.io/badge/中文文档-4578DA"></a>
</p>

![Admin Dashboard](assets/admin_dashboard.png)

## 简介
本项目基于`Pingora`——一个超高性能的网关代理库，能够支撑超大规模的请求代理服务。Pingora已被用于构建支撑Cloudflare平台核心流量处理的服务体系，多年来持续为互联网提供每秒超过4000万次请求的服务能力，目前已成为Cloudflare平台上处理相当大比例流量的技术基石。

## Http to MCP
此模式允许 `Cursor Desktop` 等客户端通过 `SSE` 与远程`Http`服务器通信，即使它本身不受支持`SSE`协议。

- 示例包含两个服务：  
  - 服务1运行于本地`127.0.0.1:8090`  
  - 服务2运行于远程`api.example.com`  
- 通过 `MCP Access Point`，服务1与服务2可以转成MCP服务，且服务1与服务2的代码都无需任何改造。
- 客户端通过 `MCP` 协议，`服务1`与`服务2`进行通信，`MCP Access Point` 自动区分`MCP`请求，并自动将请求转发到对应的后端服务。

```mermaid
graph LR
   A["Cursor Desktop"] <--> |SSE| B["MCP Access Point"]
   A2["Other Desktop"] <--> |Streamable Http| B["MCP Access Point"]
   B <--> |http 127.0.0.1:8090| C1["Existing API Server"]
   B <--> |https//api.example.com| C2["Existing API Server"]
  
   style A2 fill:#ffe6f9,stroke:#333,color:black,stroke-width:2px
   style A fill:#ffe6f9,stroke:#333,color:black,stroke-width:2px
   style B fill:#e6e6af,stroke:#333,color:black,stroke-width:2px
   style C1 fill:#e6ffe6,stroke:#333,color:black,stroke-width:2px
   style C2 fill:#e6ffd6,stroke:#333,color:black,stroke-width:2px
```
### MCP 协议支持
目前支持 `SSE` 和`Streamable HTTP`协议。
- ✅ Streamable HTTP(stateless) 2025-03-26
  -  所有服务使用 `ip:port/mcp`
  -  单服务使用 `ip:port/api/{service_id}/mcp`
  
- ✅ SSE 2024-11-05
  - 所有服务 `SSE` 使用 `ip:port/sse`
  - 单服务 `SSE` 使用 `ip:port/api/{service_id}sse`

### 支持的客户端
- ✅ [MCP Inspector](https://github.com/modelcontextprotocol/inspector)
- ✅ [Cursor Desktop](https://docs.cursor.com/context/model-context-protocol)
- ✅ [Windsurf](https://docs.windsurf.com/plugins/cascade/mcp#model-context-protocol-mcp)
- ✅ [VS Code](https://code.visualstudio.com/docs/copilot/chat/mcp-servers)
- ✅ [Trae](https://docs.trae.ai/ide/model-context-protocol)

## 核心特性

- **协议转换**：实现HTTP与MCP协议的无缝转换
- **零侵入式接入**：完全兼容现有HTTP服务，无需任何改造
- **客户端赋能**：让MCP生态客户端能够直接调用标准HTTP服务
- **轻量级代理**：极简架构设计，协议转换高效透明
- **多租户模式**：支持多租户，每个租户可独立配置MCP服务，独立 url 接入
- **运行时配置管理**：支持无重启动态配置更新
- **管理API**：提供RESTful API进行实时配置管理


## 快速开始  

### 安装方式  
```bash
# 从源码安装
git clone https://github.com/sxhxliang/mcp-access-point.git
cd mcp-access-point
# 使用 config.yaml 文件，参考config.yaml示例
cargo run -- -c config.yaml

# 使用inspector调试，先启动服务
npx @modelcontextprotocol/inspector node build/index.js
# 访问 http://127.0.0.1:6274/
# 选择 see 填入0.0.0.0:8080/sse, 点击connect就可以连接上服务啦
# 或者选择 "Streamable HTTP" 填入 0.0.0.0:8080/mcp/, 点击connect连接上服务
```

### 多租户支持
MCP接入网关支持多租户，每个租户可以配置多个MCP服务，
通过/api/{mcp-service-id}/sse （SSE）或者/api/{mcp-service-id}/mcp（Streamable HTTP）访问服务
看下面例子，比如可以通过/api/service-1/mcp访问服务2，通过/api/service-2/mcp访问服务3。
```yaml
# config.yaml 示例 (支持多服务配置)

mcps:
  - id: service-1 # /api/service-1/sse 或者 /api/service-1/mcp
    ... # 服务配置
  - id: service-2 # /api/service-2/sse 或者 /api/service-2/mcp
    ... # 服务配置
  - id: service-3 # /api/service-3/sse 或者 /api/service-3/mcp
    ... # 服务配置
```
同时获取 3 个服务的 MCP 接口
使用0.0.0.0:8080/mcp 或者0.0.0.0:8080/sse 即可访问所有服务。
具体参数请查看 config.yaml

### 参数详解：  
1. **`-c config.yaml`**
   - `-c`（或 `--config`）指定配置文件路径（`config.yaml`）。  
   - 该文件定义了 MCP 接入点要代理转换的 API。

### 使用config.yaml示例
配置文件支持多租户模式，每个MCP服务可以独立配置上游服务和路由规则。主要配置项包括：

1. **mcps** - MCP服务列表
   - `id`: 服务唯一标识，用于生成访问路径(/api/{id}/sse 或 /api/{id}/mcp)
   - `upstream_id`: 关联的上游服务ID
   - `path`: OpenAPI 规范文件路径。支持本地文件（如 `config/openapi.json`）和远程 HTTP/HTTPS URL（如 `https://petstore.swagger.io/v2/swagger.json`），同时支持 JSON 与 YAML 格式。
   - `routes`: 自定义路由配置(可选)
   - `upstream`: 上游服务特定配置(可选)

2. **upstreams** - 上游服务配置
   - `id`: 上游服务ID
   - `nodes`: 后端节点地址及权重
   - `type`: 负载均衡算法(roundrobin/random/ip_hash)
   - `scheme`: 上游http 服务协议类型(http/https)
   - `pass_host`: http host 处理方式
   - `upstream_host`: 覆盖的http host值

完整配置示例：

```yaml
# config.yaml 示例 (支持多服务配置)

mcps:
  - id: service-1 # 唯一标识，可通过 /api/service-1/sse 或 /api/service-1/mcp 独立访问
    upstream_id: 1
    path: config/openapi_for_demo_patch1.json # 本地OpenAPI规范文件路径

  - id: service-2 # 唯一标识
    upstream_id: 2
    path: https://petstore.swagger.io/v2/swagger.json  # 支持远程OpenAPI规范

  - id: service-3 
    upstream_id: 3
    routes: # 自定义路由配置
      - id: 1
        operation_id: get_weather # 操作标识符
        uri: /points/{latitude},{longitude} # 路径匹配规则
        method: GET
        meta:
          name: 获取天气
          description: 根据经纬度获取天气信息
          inputSchema: # 输入参数校验（可选）
            type: object
            required: # 必填字段
              - latitude
              - longitude
            properties: # 字段定义
              latitude:
                type: number
                minimum: -90   # 纬度最小值
                maximum: 90    # 纬度最大值
              longitude:
                type: number
                minimum: -180  # 经度最小值
                maximum: 180   # 经度最大值

upstreams: # 上游服务配置（必填）
  - id: 1
    headers: # 自定义请求头
      X-API-Key: "12345-abcdef"
      Authorization: "Bearer token123"
      User-Agent: "MyApp/1.0"
      Accept: "application/json"
    nodes: # 后端节点（支持IP或域名）
      "127.0.0.1:8090": 1 # 格式：地址:权重

  - id: 2 
    nodes:
      "127.0.0.1:8091": 1

  - id: 3 
    nodes:
      "api.weather.gov": 1
    type: roundrobin    # 负载均衡算法（支持轮询/随机/IP哈希）
    scheme: https       # 协议类型（支持http/https）
    pass_host: rewrite  # Host头处理方式（设置为rewrite时使用upstream_host覆盖）
    upstream_host: api.weather.gov # 覆盖的Host头值
```

要使用配置文件运行 MCP 接入网关，请按运行以下命令:
```bash
cargo run -- -c config.yaml
```

## 使用Docker运行

### 使用已经构建好的docker镜像

```bash
# 注意：将 /path/to/your/config.yaml 替换为你本地文件的实际路径
docker run -d --name mcp-access-point --rm \
  -p 8080:8080 \
  -e port=8080 \
  -v /path/to/your/config.yaml:/app/config/config.yaml \
  ghcr.io/sxhxliang/mcp-access-point:main
```

### 构建Docker镜像（可选，如果你想本地构建）
- 确保已安装Docker
- 运行以下命令
```bash
# 克隆仓库
git clone https://github.com/sxhxliang/mcp-access-point.git
cd mcp-access-point

# 构建Docker镜像
docker build -t sxhxliang/mcp-access-point:latest .
```

- 运行Docker容器
```bash
# 使用环境变量配置
# 注意：将 /path/to/your/config.yaml 替换为你本地文件的实际路径
docker run -d --name mcp-access-point --rm \
  -p 8080:8080 \
  -e port=8080 \
  -v /path/to/your/config.yaml:/app/config/config.yaml \
  sxhxliang/mcp-access-point:latest
```

### 环境变量说明
- `port`: MCP接入网关监听端口，默认为8080

## 典型应用场景  

- **渐进式架构迁移**：帮助HTTP服务逐步过渡到MCP架构体系  
- **混合架构支持**：在MCP生态中复用现有HTTP基础设施  
- **协议兼容方案**：构建同时支持双协议体系的混合系统  

**典型案例**：  
当采用MCP协议的AI客户端需要对接企业遗留的HTTP微服务时，MCP接入网关可作为中间层，实现协议的无缝转换。


非常感谢 [@limcheekin](https://github.com/limcheekin)
写了一篇文章介绍了一个实际例子，https://limcheekin.medium.com/building-your-first-no-code-mcp-server-the-fabric-integration-story-90da58cdbe1f

## 运行时配置管理

MCP Access Point 现在支持通过 RESTful 管理 API 进行动态配置管理，允许您在不重启服务的情况下更新配置。

### 管理 API 功能特性

- **实时配置更新**：动态修改上游服务器、服务、路由等资源配置
- **依赖关系验证**：自动验证资源间依赖关系，防止误操作
- **批量操作**：原子性执行多个配置变更
- **配置验证**：提供干跑模式，在应用前验证配置变更
- **资源统计**：监控和跟踪配置状态

### 管理 API 配置

在 `config.yaml` 中添加以下配置来启用管理 API：

```yaml
access_point:
  admin:
    address: "127.0.0.1:8081"  # 管理 API 监听地址
    api_key: "your-api-key"    # 可选的 API 密钥认证
```

### 管理 API 接口

#### 资源管理
- `GET /admin/resources` - 获取资源统计和摘要信息
- `GET /admin/resources/{type}` - 列出指定类型的所有资源
- `GET /admin/resources/{type}/{id}` - 获取特定资源
- `POST /admin/resources/{type}/{id}` - 创建新资源
- `PUT /admin/resources/{type}/{id}` - 更新现有资源
- `DELETE /admin/resources/{type}/{id}` - 删除资源

#### 高级操作
- `POST /admin/validate/{type}/{id}` - 验证资源配置
- `POST /admin/batch` - 执行批量操作
- `POST /admin/reload/{type}` - 重载指定的资源类型
- `POST /admin/reload/config` - 从配置文件重载全量配置（默认 `config.yaml`）。支持可选 JSON 请求体：`{ "config_path": "path/to/config.yaml" }`

#### 支持的资源类型
- `upstreams` - 后端服务器配置
- `services` - 服务定义
- `routes` - 路由规则
- `global_rules` - 全局插件规则
- `mcp_services` - MCP 服务配置
- `ssls` - SSL 证书配置

### 管理 API 使用示例

#### 创建新的上游服务器
```bash
curl -X POST http://localhost:8081/admin/resources/upstreams/my-upstream \
  -H "Content-Type: application/json" \
  -d '{
    "id": "my-upstream",
    "type": "RoundRobin",
    "nodes": ["127.0.0.1:8001", "127.0.0.1:8002"],
    "timeout": {
      "connect": 5,
      "read": 10,
      "send": 10
    }
  }'
```

#### 创建服务
```bash
curl -X POST http://localhost:8081/admin/resources/services/my-service \
  -H "Content-Type: application/json" \
  -d '{
    "id": "my-service",
    "upstream_id": "my-upstream",
    "hosts": ["api.example.com"]
  }'
```

#### 批量操作
```bash
curl -X POST http://localhost:8081/admin/batch \
  -H "Content-Type: application/json" \
  -d '{
    "dry_run": false,
    "operations": [
      {
        "operation_type": "create",
        "resource_type": "upstreams",
        "resource_id": "batch-upstream",
        "data": {
          "id": "batch-upstream",
          "type": "Random",
          "nodes": ["192.168.1.10:8080"]
        }
      },
      {
        "operation_type": "create",
        "resource_type": "services",
        "resource_id": "batch-service",
        "data": {
          "id": "batch-service",
          "upstream_id": "batch-upstream"
        }
      }
    ]
  }'
```

#### 获取资源统计
```bash
curl http://localhost:8081/admin/resources
```
### 管理面板（Admin Dashboard）

![管理面板](assets/admin_dashboard.png)

- 访问地址：`GET /admin`（内置页面 `static/admin_dashboard.html`）。
- 概览卡片固定顺序渲染以下 6 类资源，避免刷新时位置变化：
  1）`mcp_services`，2）`ssls`，3）`global_rules`，4）`routes`，5）`upstreams`，6）`services`。
- 每张卡片展示 `count`，并将 `last_updated` 进行本地时间格式化显示。

#### 从文件重载配置
```bash
# 使用默认的 config.yaml
curl -X POST http://localhost:8081/admin/reload/config \
  -H "Content-Type: application/json" \
  -H "x-api-key: your-api-key"

# 或者指定不同的配置路径
curl -X POST http://localhost:8081/admin/reload/config \
  -H "Content-Type: application/json" \
  -H "x-api-key: your-api-key" \
  -d '{"config_path": "./config.yaml"}'
```

### 测试管理 API

使用提供的测试脚本验证管理 API 功能：

```bash
# 赋予测试脚本执行权限
chmod +x test-admin-api.sh

# 运行全面的 API 测试
./test-admin-api.sh
```

详细的管理 API 文档请参见 [RUNTIME_CONFIG_API.md](./RUNTIME_CONFIG_API.md)。

## 贡献指南
1. Fork 这个仓库。
2. 创建一个分支，并提交你的修改。
3. 创建一个 Pull Request，并等待合并。
4. 确保你的代码遵循 Rust 的编码规范。



