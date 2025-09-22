#!/bin/bash

# 测试 MCP Access Point 增强版 Admin API
# 本脚本用于测试运行时配置管理功能（无需 etcd）

API_BASE="http://localhost:9090/admin"
HEADER_JSON="Content-Type: application/json"

echo "========================================="
echo "MCP Access Point Admin API 测试脚本"
echo "========================================="
echo ""

# 1. 获取资源统计
echo "1. 获取资源统计信息..."
curl -X GET "$API_BASE/resources" 2>/dev/null | jq '.'
echo ""

# 2. 列出所有上游服务器
echo "2. 列出所有上游服务器..."
curl -X GET "$API_BASE/resources/upstreams" 2>/dev/null | jq '.'
echo ""

# 3. 创建新的上游服务器
echo "3. 创建新的上游服务器 (backend-2)..."
curl -X POST "$API_BASE/resources/upstreams/backend-2" \
  -H "$HEADER_JSON" \
  -d '{
    "id": "backend-2",
    "type": "RoundRobin",
    "nodes": ["127.0.0.1:8001", "127.0.0.1:8002"],
    "timeout": {
      "connect": 5,
      "read": 10,
      "send": 10
    }
  }' 2>/dev/null | jq '.'
echo ""

# 4. 获取特定上游服务器
echo "4. 获取上游服务器 backend-2..."
curl -X GET "$API_BASE/resources/upstreams/backend-2" 2>/dev/null | jq '.'
echo ""

# 5. 创建服务
echo "5. 创建服务 (api-service)..."
curl -X POST "$API_BASE/resources/services/api-service" \
  -H "$HEADER_JSON" \
  -d '{
    "id": "api-service",
    "upstream_id": "backend-2",
    "hosts": ["api.example.com"],
    "plugins": {}
  }' 2>/dev/null | jq '.'
echo ""

# 6. 创建路由
echo "6. 创建路由 (api-route)..."
curl -X POST "$API_BASE/resources/routes/api-route" \
  -H "$HEADER_JSON" \
  -d '{
    "id": "api-route",
    "service_id": "api-service",
    "uris": ["/v1/*", "/v2/*"],
    "methods": ["GET", "POST", "PUT"],
    "priority": 200
  }' 2>/dev/null | jq '.'
echo ""

# 7. 验证资源（测试依赖检查）
echo "7. 验证资源 - 测试无效的服务引用..."
curl -X POST "$API_BASE/validate/routes/test-invalid" \
  -H "$HEADER_JSON" \
  -d '{
    "id": "test-invalid",
    "service_id": "non-existent-service",
    "uris": ["/test"]
  }' 2>/dev/null | jq '.'
echo ""

# 8. 批量操作
echo "8. 批量操作示例..."
curl -X POST "$API_BASE/batch" \
  -H "$HEADER_JSON" \
  -d '{
    "dry_run": false,
    "operations": [
      {
        "operation_type": "create",
        "resource_type": "upstreams",
        "resource_id": "batch-upstream-1",
        "data": {
          "id": "batch-upstream-1",
          "type": "Random",
          "nodes": ["192.168.1.10:8080"]
        }
      },
      {
        "operation_type": "create",
        "resource_type": "services",
        "resource_id": "batch-service-1",
        "data": {
          "id": "batch-service-1",
          "upstream_id": "batch-upstream-1",
          "hosts": ["batch.example.com"]
        }
      }
    ]
  }' 2>/dev/null | jq '.'
echo ""

# 9. 更新资源
echo "9. 更新上游服务器 backend-2..."
curl -X PUT "$API_BASE/resources/upstreams/backend-2" \
  -H "$HEADER_JSON" \
  -d '{
    "id": "backend-2",
    "type": "RoundRobin",
    "nodes": ["127.0.0.1:8001", "127.0.0.1:8002", "127.0.0.1:8003"],
    "timeout": {
      "connect": 10,
      "read": 20,
      "send": 20
    }
  }' 2>/dev/null | jq '.'
echo ""

# 10. 列出所有路由
echo "10. 列出所有路由..."
curl -X GET "$API_BASE/resources/routes" 2>/dev/null | jq '.'
echo ""

# 11. 删除资源（测试依赖保护）
echo "11. 尝试删除被引用的上游 backend-2..."
curl -X DELETE "$API_BASE/resources/upstreams/backend-2" 2>/dev/null | jq '.'
echo ""

# 12. 删除路由
echo "12. 删除路由 api-route..."
curl -X DELETE "$API_BASE/resources/routes/api-route" 2>/dev/null | jq '.'
echo ""

# 13. 删除服务
echo "13. 删除服务 api-service..."
curl -X DELETE "$API_BASE/resources/services/api-service" 2>/dev/null | jq '.'
echo ""

# 14. 再次删除上游（现在应该成功）
echo "14. 删除上游 backend-2..."
curl -X DELETE "$API_BASE/resources/upstreams/backend-2" 2>/dev/null | jq '.'
echo ""

# 15. 测试资源类型重新加载
echo "15. 重新加载上游配置..."
curl -X POST "$API_BASE/reload/upstreams" 2>/dev/null | jq '.'
echo ""

# 16. 测试全配置重新加载（使用默认路径）
echo "16. 重新加载完整配置（默认路径）..."
curl -X POST "$API_BASE/reload/config" 2>/dev/null | jq '.'
echo ""

# 17. 测试全配置重新加载（指定路径）
echo "17. 重新加载完整配置（指定路径）..."
curl -X POST "$API_BASE/reload/config" \
  -H "$HEADER_JSON" \
  -d '{
    "config_path": "config-test.yaml"
  }' 2>/dev/null | jq '.'
echo ""

# 18. 最终资源统计
echo "18. 最终资源统计..."
curl -X GET "$API_BASE/resources" 2>/dev/null | jq '.'
echo ""

echo "========================================="
echo "测试完成！"
echo "========================================="