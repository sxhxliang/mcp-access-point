use async_trait::async_trait;
use http::header::AsHeaderName;
use http::{header, HeaderValue};
use http::{header::CONTENT_TYPE, Method, Response, StatusCode};
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

use super::validate::*;
use crate::config::{AccessPointConfig, Admin, EtcdClientWrapper};

type RequestParams = BTreeMap<String, String>;
type ResponseResult = Result<Response<Vec<u8>>, String>;
type HttpRouterHandler = Pin<Box<dyn Future<Output = ResponseResult> + Send + 'static>>;

#[derive(Serialize, Deserialize)]
struct ValueWrapper<T> {
    value: T,
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
                // 如果 content_type 无法转换为 HeaderValue，可以选择日志记录或忽略
                log::error!("Invalid content type: {ct}");
            }
        }

        builder.body(body).unwrap()
    }

    pub fn error(status: StatusCode, message: &str) -> Response<Vec<u8>> {
        Response::builder()
            .status(status)
            .body(message.as_bytes().to_vec())
            .unwrap()
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

// Example
async fn list_resources(_req: RequestData) -> Result<Response<Vec<u8>>, String> {
    // log::debug!("list_resources: {:?}", req);
    let res = Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/html; charset=utf-8")
        .body("Method allowed".as_bytes().to_vec())
        .unwrap();
    Ok(res)
}

async fn put_resource_handle(req: RequestData) -> Result<Response<Vec<u8>>, String> {
    validate_content_type(&req)?;

    // let body_data = req.body_data.clone();
    let resource_type = req
        .params
        .get("resource")
        .ok_or_else(|| "MissingParameter resource".to_string())?;
    let key = format!(
        "{}/{}",
        resource_type,
        req.params
            .get("id")
            .ok_or_else(|| "MissingParameter id".to_string())?
    );

    // validate_resource(resource_type, &req.body_data)?;
    req.etcd
        .lock()
        .await
        .put(&key, req.body_data)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseHelper::success(Vec::new(), None))
}

async fn get_resource_handle(req: RequestData) -> Result<Response<Vec<u8>>, String> {
    let resource_type = req
        .params
        .get("resource")
        .ok_or_else(|| "Missing parameter".to_string())?;

    let key = format!(
        "{}/{}",
        resource_type,
        req.params
            .get("id")
            .ok_or_else(|| "Missing upstream configuration".to_string())?
    );

    match req.etcd.lock().await.get(&key).await {
        Err(e) => Err(e.to_string()),
        Ok(Some(value)) => {
            let json_value: serde_json::Value =
                serde_json::from_slice(&value).map_err(|e| format!("Invalid JSON data: {e}"))?;

            // let wrapper = ValueWrapper { value: json_value };
            let json_vec = serde_json::to_vec(&json_value).map_err(|e| e.to_string())?;
            Ok(ResponseHelper::success(json_vec, Some("application/json")))
        }
        Ok(None) => Err("Resource not found".to_string()),
    }
}

async fn delete_resource_handle(req: RequestData) -> Result<Response<Vec<u8>>, String> {
    let key = format!(
        "{}/{}",
        req.params
            .get("resource")
            .ok_or_else(|| "MissingParameter resource".to_string())?,
        req.params
            .get("id")
            .ok_or_else(|| "MissingParameter id".to_string())?
    );

    req.etcd
        .lock()
        .await
        .delete(&key)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseHelper::success(Vec::new(), None))
}
/// AdminHttpApp
pub struct AdminHttpApp {
    config: Admin,
    etcd: EtcdClientWrapper,
    router: Router<HashMap<Method, AsyncHandlerWithArg<RequestData>>>,
}

impl AdminHttpApp {
    /// new admin http app
    pub fn new(config: &AccessPointConfig) -> Self {
        let mut this = Self {
            config: config.admin.clone().unwrap(),
            etcd: EtcdClientWrapper::new(config.etcd.clone().unwrap()),
            router: Router::new(),
        };

        let put_handler: AsyncHandlerWithArg<RequestData> = AsyncHandlerWithArg::new(
            Method::PUT,
            "/admin/{resource}/{id}".to_string(),
            put_resource_handle,
        );

        let get_handler: AsyncHandlerWithArg<RequestData> = AsyncHandlerWithArg::new(
            Method::GET,
            "/admin/{resource}/{id}".to_string(),
            get_resource_handle,
        );

        let delete_handler: AsyncHandlerWithArg<RequestData> = AsyncHandlerWithArg::new(
            Method::DELETE,
            "/admin/{resource}/{id}".to_string(),
            delete_resource_handle,
        );

        this.route(put_handler);
        this.route(get_handler);
        this.route(delete_handler);
        this
    }

    fn route(&mut self, handler: AsyncHandlerWithArg<RequestData>) -> &mut Self {
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
    pub fn admin_http_service(cfg: &AccessPointConfig) -> Service<AdminHttpApp> {
        let app = AdminHttpApp::new(cfg);
        let addr = &app.config.address.to_string();
        let mut service = Service::new("Admin HTTP".to_string(), app);
        service.add_tcp(addr);
        service
    }
}

async fn read_request_body(http_session: &mut ServerSession) -> Result<Vec<u8>, String> {
    let body_data = match http_session.read_request_body().await {
        Ok(Some(body_data)) => body_data.to_vec(),
        Ok(None) => vec![], // done
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
                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(e.as_bytes().to_vec())
                    .unwrap();
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

                    let request_data = RequestData {
                        etcd: Arc::new(Mutex::new(self.etcd.clone())),
                        params,
                        header: http_session.req_header().clone(),
                        body_data,
                    };
                    match handler.call(request_data).await {
                        Ok(resp) => resp,
                        Err(e) => {
                            log::error!("Handler execution failed: {e:?}");
                            Response::builder()
                                .status(StatusCode::BAD_REQUEST)
                                .body(e.as_bytes().to_vec())
                                .unwrap()
                        }
                    }
                }
                // 405 Method Not Allowed
                None => Response::builder()
                    .status(StatusCode::METHOD_NOT_ALLOWED)
                    .body("Method not allowed".as_bytes().to_vec())
                    .unwrap(),
            },
            // 404 Not Found
            Err(_) => Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body("Not Found".as_bytes().to_vec())
                .unwrap(),
        }
    }
}
