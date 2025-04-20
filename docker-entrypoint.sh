#!/bin/bash
set -e

# 默认端口
PORT=${port:-8080}
# yaml 配置文件路径
CONFIG_FILE=${config_file:-/app/config/config.yaml}

# 检查是否提供了config_file环境变量，并确保文件存在
if [ -n "$config_file" ]; then
    # 如果是宿主机路径，会通过volume挂载，所以直接使用
    if [ -f "$config_file" ]; then
        echo "Using Config file from: $config_file"
        CONFIG_FILE=$config_file
    else
        echo "Warning: Config file not found at $config_file"
        echo "Please make sure you've mounted the file correctly."
        echo "Example: -v /path/on/host/config.yaml:$config_file"
        exit 1
    fi
fi

# 启动应用程序
echo "Starting MCP Access Point..."
echo "Port: $PORT"
echo "Config file: $CONFIG_FILE"

exec /app/access-point -c "$CONFIG_FILE"