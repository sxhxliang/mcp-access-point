
我已经根据提供的OpenAPI规范使用FastAPI创建了完整的Petstore API实现。

## 主要功能

**Pet 管理接口:**
- 添加新宠物 (`POST /pet`)
- 更新宠物信息 (`PUT /pet`)
- 根据状态查找宠物 (`GET /pet/findByStatus`)
- 根据标签查找宠物 (`GET /pet/findByTags`)
- 根据ID获取宠物 (`GET /pet/{petId}`)
- 上传宠物图片 (`POST /pet/{petId}/uploadImage`)
- 更新宠物表单数据 (`POST /pet/{petId}`)
- 删除宠物 (`DELETE /pet/{petId}`)

**Store 订单接口:**
- 获取库存信息 (`GET /store/inventory`)
- 下订单 (`POST /store/order`)
- 根据ID获取订单 (`GET /store/order/{orderId}`)
- 删除订单 (`DELETE /store/order/{orderId}`)

**User 用户接口:**
- 创建用户 (`POST /user`)
- 批量创建用户 (`POST /user/createWithArray`, `POST /user/createWithList`)
- 用户登录 (`GET /user/login`)
- 用户登出 (`GET /user/logout`)
- 获取用户信息 (`GET /user/{username}`)
- 更新用户 (`PUT /user/{username}`)
- 删除用户 (`DELETE /user/{username}`)

## 技术特点

1. **完整的数据模型**: 使用Pydantic定义了所有的数据模型，包括Pet、User、Order等
2. **枚举类型**: 定义了PetStatus和OrderStatus枚举
3. **数据验证**: 使用Field进行字段验证和文档说明
4. **内存数据库**: 使用字典模拟数据存储（生产环境中应该使用真实数据库）
5. **错误处理**: 包含适当的HTTP状态码和错误消息
6. **API文档**: 自动生成OpenAPI文档
7. **示例数据**: 初始化了一些示例数据用于测试

## 运行方式

```bash
pip install fastapi uvicorn python-multipart
python main.py
```

启动后访问：
- API文档: http://localhost:8090/docs
- ReDoc文档: http://localhost:8090/redoc
- API根路径: http://localhost:8090
