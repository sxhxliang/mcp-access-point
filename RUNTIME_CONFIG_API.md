# 运行时配置管理 API 文档

## 概述

本文档描述了 MCP Access Point 的运行时配置管理功能，允许在不重启服务的情况下动态更新配置。

## 全局变量管理

### 涉及的全局变量映射

- **UPSTREAM_MAP** - 上游服务器配置
- **SERVICE_MAP** - 服务配置
- **GLOBAL_RULE_MAP** - 全局规则配置
- **ROUTE_MAP** - 路由配置
- **MCP_SERVICE_MAP** - MCP 服务配置
- **SSL_MAP** - SSL 证书配置
- **MCP_SERVICE_TOOLS_MAP** - MCP 工具列表
- **MCP_ROUTE_META_INFO_MAP** - MCP 路由元信息

## 资源管理器功能

### ResourceManager

统一的资源管理器提供以下核心功能：

1. **CRUD 操作**
   - 创建资源: `create_resource()`
   - 读取资源: `get_resource()`
   - 更新资源: `update_resource()`
   - 删除资源: `delete_resource()`
   - 列出资源: `list_resources()`

2. **批量操作**
   - `execute_batch_operations()` - 支持批量创建、更新、删除
   - 支持 dry-run 模式进行预检

3. **验证功能**
   - 资源格式验证
   - 依赖关系检查
   - 删除前的引用检查
   - 循环依赖检测（待实现）

4. **配置变更通知**
   - 支持注册监听器
   - 自动广播配置变更事件

## 资源类型定义

### ResourceType 枚举

```rust
pub enum ResourceType {
    Upstreams,      // 上游服务器
    Services,       // 服务
    GlobalRules,    // 全局规则
    Routes,         // 路由
    McpServices,    // MCP服务
    Ssls,          // SSL证书
}
```

### 依赖关系

- **Upstreams**: 无依赖
- **Services**: 依赖 Upstreams
- **Routes**: 依赖 Upstreams 和 Services
- **McpServices**: 依赖 Upstreams
- **GlobalRules**: 无依赖
- **Ssls**: 无依赖

## 资源验证器

### ValidationResult 结构

```rust
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
}
```

### 验证类型

1. **格式验证** - JSON 格式和字段验证
2. **依赖验证** - 引用的资源是否存在
3. **约束验证** - 业务规则检查
4. **删除验证** - 检查是否被其他资源引用

## Admin API 使用示例

### 创建上游服务器

```bash
# 创建上游配置
curl -X PUT http://localhost:8081/admin/upstreams/backend-1 \
  -H "Content-Type: application/json" \
  -d '{
    "id": "backend-1",
    "nodes": ["127.0.0.1:8001", "127.0.0.1:8002"],
    "type": "RoundRobin",
    "timeout": {
      "connect": 5,
      "read": 10,
      "send": 10
    }
  }'
```

### 创建服务

```bash
# 创建服务配置
curl -X PUT http://localhost:8081/admin/services/api-service \
  -H "Content-Type: application/json" \
  -d '{
    "id": "api-service",
    "upstream_id": "backend-1",
    "hosts": ["api.example.com"],
    "plugins": {
      "cors": {
        "allow_origins": "*",
        "allow_methods": ["GET", "POST"]
      }
    }
  }'
```

### 创建路由

```bash
# 创建路由配置
curl -X PUT http://localhost:8081/admin/routes/api-route \
  -H "Content-Type: application/json" \
  -d '{
    "id": "api-route",
    "service_id": "api-service",
    "uris": ["/api/v1/*"],
    "methods": ["GET", "POST"],
    "priority": 100
  }'
```

### 批量操作

```bash
# 批量创建资源
curl -X POST http://localhost:8081/admin/resources/batch \
  -H "Content-Type: application/json" \
  -d '{
    "dry_run": false,
    "operations": [
      {
        "operation_type": "create",
        "resource_type": "upstreams",
        "resource_id": "upstream-1",
        "data": {
          "nodes": ["127.0.0.1:8001"]
        }
      },
      {
        "operation_type": "create",
        "resource_type": "services",
        "resource_id": "service-1",
        "data": {
          "upstream_id": "upstream-1"
        }
      }
    ]
  }'
```

### 验证配置

```bash
# 验证配置（dry-run）
curl -X POST http://localhost:8081/admin/resources/validate \
  -H "Content-Type: application/json" \
  -d '{
    "resource_type": "routes",
    "resource_id": "test-route",
    "data": {
      "service_id": "non-existent-service",
      "uris": ["/test"]
    }
  }'
```

### 获取资源统计

```bash
# 获取所有资源的统计信息
curl http://localhost:8081/admin/resources
```

响应示例：
```json
{
  "stats": {
    "mcp_services": {
      "resource_type": "mcp_services",
      "count": 3,
      "last_updated": { "secs_since_epoch": 1759219032, "nanos_since_epoch": 410206700 }
    },
    "ssls": {
      "resource_type": "ssls",
      "count": 0,
      "last_updated": { "secs_since_epoch": 1759219032, "nanos_since_epoch": 410210500 }
    },
    "global_rules": {
      "resource_type": "global_rules",
      "count": 0,
      "last_updated": { "secs_since_epoch": 1759219032, "nanos_since_epoch": 410195700 }
    },
    "routes": {
      "resource_type": "routes",
      "count": 1,
      "last_updated": { "secs_since_epoch": 1759219032, "nanos_since_epoch": 410200300 }
    },
    "upstreams": {
      "resource_type": "upstreams",
      "count": 3,
      "last_updated": { "secs_since_epoch": 1759219032, "nanos_since_epoch": 410175400 }
    },
    "services": {
      "resource_type": "services",
      "count": 0,
      "last_updated": { "secs_since_epoch": 1759219032, "nanos_since_epoch": 410187600 }
    }
  },
  "total_resources": 7
}
```
Notes:

- The `last_updated` field is serialized from Rust `SystemTime` as `{ secs_since_epoch, nanos_since_epoch }`.
- The `stats` object keys are fixed resource types: `mcp_services`, `ssls`, `global_rules`, `routes`, `upstreams`, `services`.

## 配置热重载

某些资源类型支持热重载：

- **GlobalRules**: 更新后自动重载全局插件
- **Routes**: 更新后自动重载路由匹配器
- **Ssls**: 更新后自动重载 SSL 证书匹配器

## 错误处理

### 常见错误码

- **400 Bad Request**: 配置格式错误或验证失败
- **404 Not Found**: 资源不存在
- **409 Conflict**: 资源冲突（如删除被引用的资源）
- **500 Internal Server Error**: 服务器内部错误

### 错误响应格式

```json
{
  "success": false,
  "message": "Validation failed: Referenced upstream 'backend-1' does not exist",
  "resource_type": "services",
  "resource_id": "api-service",
  "timestamp": "2025-01-21T10:30:00Z"
}
```

## 最佳实践

1. **使用 dry-run 模式**: 在实际执行前先验证配置
2. **批量操作顺序**: 确保依赖资源先创建
3. **定期备份**: 定期导出配置进行备份
4. **监控变更**: 使用配置变更监听器记录审计日志
5. **逐步迁移**: 逐步将静态配置迁移到动态配置

## 注意事项

1. **健康检查**: Upstream 更新会重启健康检查
2. **连接池**: 更新 Upstream 会影响现有连接
3. **插件加载**: 某些插件可能需要重启才能完全生效
4. **性能影响**: 频繁的配置更新可能影响性能

## 未来规划

- [ ] 支持配置版本控制
- [ ] 实现配置回滚功能
- [ ] 添加配置变更审计日志
- [ ] 支持配置导入导出
- [ ] 实现配置模板功能
- [ ] 支持配置差异对比