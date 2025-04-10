#!/bin/bash
set -e

# 默认端口
PORT=${port:-8080}
# 默认上游服务
UPSTREAM=${upstream:-localhost:8090}
# OpenAPI文件路径
OPENAPI_JSON=${openapi_json:-/app/config/openapi.json}

# 检查是否提供了openapi_json环境变量，并确保文件存在
if [ -n "$openapi_json" ]; then
    # 如果是宿主机路径，会通过volume挂载，所以直接使用
    if [ -f "$openapi_json" ]; then
        echo "Using OpenAPI file from: $openapi_json"
        OPENAPI_JSON=$openapi_json
    else
        echo "Warning: OpenAPI file not found at $openapi_json"
        echo "Please make sure you've mounted the file correctly."
        echo "Example: -v /path/on/host/openapi.json:$openapi_json"
        exit 1
    fi
fi

# 启动应用程序
echo "Starting MCP Access Point..."
echo "Port: $PORT"
echo "Upstream: $UPSTREAM"
echo "OpenAPI file: $OPENAPI_JSON"

exec /app/mcp-access-point -f "$OPENAPI_JSON" -p "$PORT" -u "$UPSTREAM" 