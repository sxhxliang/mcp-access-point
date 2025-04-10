# MCP Access Point

MCP Access Point is a lightweight gateway tool designed to bridge traditional HTTP services with MCP (Model Context Protocol) clients. It enables seamless interaction between MCP clients and existing HTTP services without requiring any modifications to the server-side interface code.

<p align="center">
  <a href="./README.md"><img alt="README in English" src="https://img.shields.io/badge/English-d9d9d9"></a>
  <a href="./README_CN.md"><img alt="简体中文版自述文件" src="https://img.shields.io/badge/简体中文-d9d9d9"></a>

</p>

## Quick Start  

### Installation Method  
```bash  
# Install from source  
git clone https://github.com/sxhxliang/mcp-access-point.git  
cd mcp-access-point  
# Pass the openapi.json file path, mcp port number, and upstream service address  
cargo run -- -f openapi_for_demo.json -p 8080 -u localhost:8090  
# Use inspector for debugging. First, start the service.  
npx @modelcontextprotocol/inspector@0.8.1 node build/index.js  
# Access http://127.0.0.1:6274/  
# Select "see," fill in 0.0.0.0:8080/sse, and click "connect" to link to the service.  
```

### Breakdown of Arguments:  
1. **`-f openapi_for_demo.json`**  
   - `-f` (or `--file`) specifies the path to the OpenAPI specification file (`openapi_for_demo.json`).  
   - This file defines the API that the MCP (Model Context Protocol) access point will use.  

2. **`-p 8080`**  
   - `-p` (or `--port`) sets the port number (`8080`) on which the MCP access point will listen for incoming requests.  

3. **`-u localhost:8090`**  
   - `-u` (or `--upstream`) defines the upstream service address (`localhost:8090`).  
   - The MCP access point will forward requests to this address after processing them.  

Key Characteristics:
- Protocol Conversion: Translates between HTTP and MCP protocols bidirectionally
- Zero Modification: Works with existing HTTP services as-is
- Client Enablement: Allows MCP clients to consume standard HTTP services
- Lightweight Proxy: Minimal overhead with clean protocol translation

## Running with Docker

### Build Docker Image (Optional, if you want to build locally)
```bash
# Clone the repository
git clone https://github.com/sxhxliang/mcp-access-point.git
cd mcp-access-point

# Build Docker image
docker build -t kames2025/mcp-access-point:latest .
```

### Pull and Run Docker Container
```bash
# Using environment variables for configuration (upstream service running on host)
# Note: Replace /path/to/your/openapi.json with the actual path to your local OpenAPI file
# Note: The upstream address uses host.docker.internal to point to the host machine. If this doesn't work, try the host's LAN IP address.
docker run -d --name mcp-access-point --rm \
  -p 8080:8080 \
  -e port=8080 \
  -e upstream=host.docker.internal:8090 \
  -e openapi_json=/app/config/openapi.json \
  -v /path/to/your/openapi.json:/app/config/openapi.json \
  kames2025/mcp-access-point:latest

# Or specify the openapi_json environment variable directly
docker run -d --name mcp-access-point --rm \
  -p 8080:8080 \
  -e port=8080 \
  -e upstream=host.docker.internal:8090 \
  -e openapi_json=/app/config/openapi.json \
  -v /path/to/your/openapi.json:/app/config/openapi.json \
  kames2025/mcp-access-point:latest
```

### Environment Variables
- `port`: MCP access point listening port, default is 8080
- `upstream`: Upstream service address, default is localhost:8090
- `openapi_json`: Path to the OpenAPI specification file, default is /app/config/openapi.json

The solution is particularly valuable for:
- Gradually migrating HTTP services to MCP architecture
- Enabling MCP-based systems to leverage existing HTTP infrastructure
- Building hybrid systems that need to support both protocols

Example use case:
An AI service with MCP-native clients needs to integrate with legacy HTTP-based microservices. The MCP Access Point sits between them, handling protocol translation transparently.

Would you like me to develop any particular aspect of this description further, such as technical architecture or specific protocol conversion details?
