use async_trait::async_trait;
use http::header::AsHeaderName;
use http::{header, HeaderValue};
use http::{Method, Response, StatusCode};
use matchit::{Match, Router};
use pingora::{
    apps::http_app::ServeHttp, protocols::http::ServerSession, services::listening::Service,
};
use pingora_http::RequestHeader;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
use tokio::sync::Mutex;

use super::{
    resource_manager::ResourceManager,
    resource_types::{
        BatchOperationRequest, ResourceType,
    },
};
use crate::config::{AccessPointConfig, Admin, Config, EtcdClientWrapper};

type RequestParams = BTreeMap<String, String>;
type ResponseResult = Result<Response<Vec<u8>>, String>;
type HttpRouterHandler = Pin<Box<dyn Future<Output = ResponseResult> + Send + 'static>>;

/// HTTP admin server for MCP Access Point
pub struct RequestData {
    etcd: Arc<Mutex<EtcdClientWrapper>>,
    params: RequestParams,
    header: RequestHeader,
    body_data: Vec<u8>,
}

impl RequestData {
    /// Get header value by key
    pub fn get_header<K: AsHeaderName>(&self, key: K) -> Option<&HeaderValue> {
        self.header.headers.get(key)
    }
    /// Get header value by key as string
    pub fn get_header_value(&self, key: &str) -> Option<String> {
        self.header
            .headers
            .get(key)
            .map(|v| v.to_str().unwrap_or("").to_string())
    }
}

#[derive(Serialize, Deserialize)]
struct ErrorResponse {
    success: bool,
    message: String,
}

// Unified response handler
struct ResponseHelper;

impl ResponseHelper {
    pub fn success(body: Vec<u8>, content_type: Option<&str>) -> Response<Vec<u8>> {
        let mut builder = Response::builder().status(StatusCode::OK);

        if let Some(ct) = content_type {
            if let Ok(header_value) = HeaderValue::from_str(ct) {
                builder = builder.header(header::CONTENT_TYPE, header_value);
            } else {
                log::error!("Invalid content type: {ct}");
            }
        }

        builder.body(body).unwrap()
    }

    pub fn error(status: StatusCode, message: &str) -> Response<Vec<u8>> {
        let error_response = ErrorResponse {
            success: false,
            message: message.to_string(),
        };
        let body = serde_json::to_vec(&error_response).unwrap_or_else(|_| message.as_bytes().to_vec());

        Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, "application/json")
            .body(body)
            .unwrap()
    }

    pub fn json_response<T: Serialize>(data: T) -> Response<Vec<u8>> {
        match serde_json::to_vec(&data) {
            Ok(body) => Self::success(body, Some("application/json")),
            Err(e) => Self::error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Failed to serialize response: {e}"))
        }
    }
}

struct AsyncHandlerWithArg<Arg> {
    method: Method,
    path: String,
    handler: Arc<Mutex<dyn Fn(Arg) -> HttpRouterHandler + Send + Sync>>,
}

impl<Arg: 'static> AsyncHandlerWithArg<Arg> {
    fn new<F, Fut>(method: Method, path: String, handler: F) -> Self
    where
        F: Fn(Arg) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ResponseResult> + Send + 'static,
    {
        AsyncHandlerWithArg {
            method,
            path,
            handler: Arc::new(Mutex::new(move |arg| -> HttpRouterHandler {
                Box::pin(handler(arg)) as HttpRouterHandler
            })),
        }
    }

    async fn call(&self, arg: Arg) -> Result<Response<Vec<u8>>, String> {
        let future = self.handler.lock().await(arg);
        future.await
    }
}

/// Enhanced request data with resource manager support
pub struct RequestDataEnhanced {
    etcd: Option<Arc<Mutex<EtcdClientWrapper>>>,
    resource_manager: Arc<ResourceManager>,
    params: RequestParams,
    header: RequestHeader,
    body_data: Vec<u8>,
}

impl RequestDataEnhanced {
    /// Get header value by key
    pub fn get_header<K: AsHeaderName>(&self, key: K) -> Option<&HeaderValue> {
        self.header.headers.get(key)
    }

    /// Get header value by key as string
    pub fn get_header_value(&self, key: &str) -> Option<String> {
        self.header
            .headers
            .get(key)
            .map(|v| v.to_str().unwrap_or("").to_string())
    }
}

// Handler for getting resource summary
async fn get_resources_summary(req: RequestDataEnhanced) -> Result<Response<Vec<u8>>, String> {
    let summary = req.resource_manager.get_resource_summary();
    Ok(ResponseHelper::json_response(summary))
}

// Handler for listing resources of a specific type
async fn list_resources_by_type(req: RequestDataEnhanced) -> Result<Response<Vec<u8>>, String> {
    let resource_type_str = req.params
        .get("type")
        .ok_or_else(|| "Missing resource type parameter".to_string())?;

    let resource_type = ResourceType::from_str(resource_type_str)
        .ok_or_else(|| format!("Invalid resource type: {resource_type_str}"))?;

    let resources = req.resource_manager.list_resources(resource_type);
    Ok(ResponseHelper::json_response(resources))
}

// Handler for getting a specific resource
async fn get_resource(req: RequestDataEnhanced) -> Result<Response<Vec<u8>>, String> {
    let resource_type_str = req.params
        .get("type")
        .ok_or_else(|| "Missing resource type parameter".to_string())?;

    let resource_id = req.params
        .get("id")
        .ok_or_else(|| "Missing resource id parameter".to_string())?;

    let resource_type = ResourceType::from_str(resource_type_str)
        .ok_or_else(|| format!("Invalid resource type: {resource_type_str}"))?;

    match req.resource_manager.get_resource(resource_type, resource_id) {
        Some(resource) => Ok(ResponseHelper::json_response(resource)),
        None => Ok(ResponseHelper::error(StatusCode::NOT_FOUND, &format!("Resource not found: {resource_id}"))),
    }
}

// Handler for creating a resource
async fn create_resource(req: RequestDataEnhanced) -> Result<Response<Vec<u8>>, String> {
    validate_content_type(&req)?;

    let resource_type_str = req.params
        .get("type")
        .ok_or_else(|| "Missing resource type parameter".to_string())?;

    let resource_id = req.params
        .get("id")
        .ok_or_else(|| "Missing resource id parameter".to_string())?;

    let resource_type = ResourceType::from_str(resource_type_str)
        .ok_or_else(|| format!("Invalid resource type: {resource_type_str}"))?;

    let result = req.resource_manager
        .create_resource(resource_type, resource_id.clone(), &req.body_data)
        .await?;

    if result.success {
        Ok(ResponseHelper::json_response(result))
    } else {
        Ok(ResponseHelper::error(StatusCode::BAD_REQUEST, &result.message))
    }
}

// Handler for updating a resource
async fn update_resource(req: RequestDataEnhanced) -> Result<Response<Vec<u8>>, String> {
    validate_content_type(&req)?;

    let resource_type_str = req.params
        .get("type")
        .ok_or_else(|| "Missing resource type parameter".to_string())?;

    let resource_id = req.params
        .get("id")
        .ok_or_else(|| "Missing resource id parameter".to_string())?;

    let resource_type = ResourceType::from_str(resource_type_str)
        .ok_or_else(|| format!("Invalid resource type: {resource_type_str}"))?;

    let result = req.resource_manager
        .update_resource(resource_type, resource_id.clone(), &req.body_data)
        .await?;

    if result.success {
        Ok(ResponseHelper::json_response(result))
    } else {
        Ok(ResponseHelper::error(StatusCode::BAD_REQUEST, &result.message))
    }
}

// Handler for deleting a resource
async fn delete_resource(req: RequestDataEnhanced) -> Result<Response<Vec<u8>>, String> {
    let resource_type_str = req.params
        .get("type")
        .ok_or_else(|| "Missing resource type parameter".to_string())?;

    let resource_id = req.params
        .get("id")
        .ok_or_else(|| "Missing resource id parameter".to_string())?;

    let resource_type = ResourceType::from_str(resource_type_str)
        .ok_or_else(|| format!("Invalid resource type: {resource_type_str}"))?;

    let result = req.resource_manager
        .delete_resource(resource_type, resource_id.clone())
        .await?;

    if result.success {
        Ok(ResponseHelper::json_response(result))
    } else {
        Ok(ResponseHelper::error(StatusCode::BAD_REQUEST, &result.message))
    }
}

// Handler for validating a resource
async fn validate_resource(req: RequestDataEnhanced) -> Result<Response<Vec<u8>>, String> {
    validate_content_type(&req)?;

    let resource_type_str = req.params
        .get("type")
        .ok_or_else(|| "Missing resource type parameter".to_string())?;

    let resource_id = req.params
        .get("id")
        .ok_or_else(|| "Missing resource id parameter".to_string())?;

    let resource_type = ResourceType::from_str(resource_type_str)
        .ok_or_else(|| format!("Invalid resource type: {resource_type_str}"))?;

    let validation_result = req.resource_manager
        .validate_resource(resource_type, resource_id, &req.body_data);

    Ok(ResponseHelper::json_response(validation_result))
}

// Handler for batch operations
async fn batch_operations(req: RequestDataEnhanced) -> Result<Response<Vec<u8>>, String> {
    validate_content_type(&req)?;

    let batch_request: BatchOperationRequest = serde_json::from_slice(&req.body_data)
        .map_err(|e| format!("Invalid batch request: {e}"))?;

    let result = req.resource_manager
        .execute_batch_operations(batch_request)
        .await?;

    if result.success {
        Ok(ResponseHelper::json_response(result))
    } else {
        Ok(ResponseHelper::error(StatusCode::BAD_REQUEST, &result.summary))
    }
}

// Handler for reloading a resource type
async fn reload_resource_type(req: RequestDataEnhanced) -> Result<Response<Vec<u8>>, String> {
    let resource_type_str = req.params
        .get("type")
        .ok_or_else(|| "Missing resource type parameter".to_string())?;

    let resource_type = ResourceType::from_str(resource_type_str)
        .ok_or_else(|| format!("Invalid resource type: {resource_type_str}"))?;

    let result = req.resource_manager
        .reload_resource_type(resource_type)
        .await?;

    Ok(ResponseHelper::json_response(result))
}

// Helper to validate content type
fn validate_content_type(req: &RequestDataEnhanced) -> Result<(), String> {
    match req.get_header(header::CONTENT_TYPE) {
        Some(content_type) if content_type.to_str().unwrap_or("") == "application/json" => Ok(()),
        _ => Err("Content-Type must be application/json".into()),
    }
}

/// Enhanced Admin HTTP App with resource manager
pub struct AdminHttpApp {
    config: Admin,
    etcd: Option<EtcdClientWrapper>,
    resource_manager: Arc<ResourceManager>,
    router: Router<HashMap<Method, AsyncHandlerWithArg<RequestDataEnhanced>>>,
}

impl AdminHttpApp {
    /// Create new enhanced admin app
    pub fn new(config: &Config) -> Self {
        let mut this = Self {
            config: config.access_point.admin.clone().unwrap_or_default(),
            etcd: config.access_point.etcd.as_ref().map(|e| EtcdClientWrapper::new(e.clone())),
            resource_manager: Arc::new(ResourceManager::new(config.pingora.work_stealing)),
            router: Router::new(),
        };

        // Register all routes
        this.register_routes();
        this
    }

    fn register_routes(&mut self) {
        // Resource summary
        self.route(AsyncHandlerWithArg::new(
            Method::GET,
            "/admin/resources".to_string(),
            get_resources_summary,
        ));

        // List resources by type
        self.route(AsyncHandlerWithArg::new(
            Method::GET,
            "/admin/resources/{type}".to_string(),
            list_resources_by_type,
        ));

        // Get specific resource
        self.route(AsyncHandlerWithArg::new(
            Method::GET,
            "/admin/resources/{type}/{id}".to_string(),
            get_resource,
        ));

        // Create resource
        self.route(AsyncHandlerWithArg::new(
            Method::POST,
            "/admin/resources/{type}/{id}".to_string(),
            create_resource,
        ));

        // Update resource
        self.route(AsyncHandlerWithArg::new(
            Method::PUT,
            "/admin/resources/{type}/{id}".to_string(),
            update_resource,
        ));

        // Delete resource
        self.route(AsyncHandlerWithArg::new(
            Method::DELETE,
            "/admin/resources/{type}/{id}".to_string(),
            delete_resource,
        ));

        // Validate resource
        self.route(AsyncHandlerWithArg::new(
            Method::POST,
            "/admin/validate/{type}/{id}".to_string(),
            validate_resource,
        ));

        // Batch operations
        self.route(AsyncHandlerWithArg::new(
            Method::POST,
            "/admin/batch".to_string(),
            batch_operations,
        ));

        // Reload resource type
        self.route(AsyncHandlerWithArg::new(
            Method::POST,
            "/admin/reload/{type}".to_string(),
            reload_resource_type,
        ));
    }

    fn route(&mut self, handler: AsyncHandlerWithArg<RequestDataEnhanced>) -> &mut Self {
        match self.router.at_mut(&handler.path) {
            Ok(routes) => {
                routes.value.insert(handler.method.clone(), handler);
            }
            Err(_) => {
                let mut handlers = HashMap::new();
                let path = handler.path.clone();
                handlers.insert(handler.method.clone(), handler);
                if let Err(err) = self.router.insert(path.clone(), handlers) {
                    panic!("Failed to insert path {path}: {err}");
                }
            }
        }
        self
    }

    /// Create admin http service
    pub fn admin_http_service(cfg: &Config) -> Service<AdminHttpApp> {
        let app = AdminHttpApp::new(cfg);
        let addr = app.config.address.to_string();

        // Log all available routes
        log::info!("Admin API available at {addr}");
        log::info!("Available endpoints:");
        log::info!("  GET    /admin/resources - Get resource summary");
        log::info!("  GET    /admin/resources/{{type}} - List resources by type");
        log::info!("  GET    /admin/resources/{{type}}/{{id}} - Get specific resource");
        log::info!("  POST   /admin/resources/{{type}}/{{id}} - Create resource");
        log::info!("  PUT    /admin/resources/{{type}}/{{id}} - Update resource");
        log::info!("  DELETE /admin/resources/{{type}}/{{id}} - Delete resource");
        log::info!("  POST   /admin/validate/{{type}}/{{id}} - Validate resource");
        log::info!("  POST   /admin/batch - Execute batch operations");
        log::info!("  POST   /admin/reload/{{type}} - Reload resource type");

        let mut service = Service::new("Admin HTTP Enhanced".to_string(), app);
        service.add_tcp(&addr);
        service
    }
}

async fn read_request_body(http_session: &mut ServerSession) -> Result<Vec<u8>, String> {
    let body_data = match http_session.read_request_body().await {
        Ok(Some(body_data)) => body_data.to_vec(),
        Ok(None) => vec![],
        Err(e) => return Err(e.to_string()),
    };
    Ok(body_data)
}

#[async_trait]
impl ServeHttp for AdminHttpApp {
    async fn response(&self, http_session: &mut ServerSession) -> Response<Vec<u8>> {
        http_session.set_keepalive(None);

        let body_data = match read_request_body(http_session).await {
            Ok(data) => data,
            Err(e) => {
                return ResponseHelper::error(StatusCode::BAD_REQUEST, &e);
            }
        };

        let (path, method) = {
            let req_header = http_session.req_header();
            (req_header.uri.path().to_string(), req_header.method.clone())
        };

        match self.router.at(&path) {
            Ok(Match { value, params }) => match value.get(&method) {
                Some(handler) => {
                    let params: RequestParams = params
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect();

                    let request_data = RequestDataEnhanced {
                        etcd: self.etcd.as_ref().map(|e| Arc::new(Mutex::new(e.clone()))),
                        resource_manager: self.resource_manager.clone(),
                        params,
                        header: http_session.req_header().clone(),
                        body_data,
                    };

                    match handler.call(request_data).await {
                        Ok(resp) => resp,
                        Err(e) => {
                            log::error!("Handler execution failed: {e:?}");
                            ResponseHelper::error(StatusCode::INTERNAL_SERVER_ERROR, &e)
                        }
                    }
                }
                None => ResponseHelper::error(StatusCode::METHOD_NOT_ALLOWED, "Method not allowed"),
            },
            Err(_) => ResponseHelper::error(StatusCode::NOT_FOUND, "Not found"),
        }
    }
}