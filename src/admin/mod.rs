use async_trait::async_trait;
use http::{header::CONTENT_TYPE, Method, Response, StatusCode};
use matchit::{Match, Router};
use pingora::{
    apps::http_app::ServeHttp, protocols::http::ServerSession, services::listening::Service,
};
use std::future::Future;
use std::pin::Pin;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
};

use crate::config::Admin;

type RequestParams = BTreeMap<String, String>;
type ResponseResult = Result<Response<Vec<u8>>, String>;
type HttpRouterHandler = Pin<Box<dyn Future<Output = ResponseResult> + Send + 'static>>;

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
        let future = self.handler.lock().unwrap()(arg);
        future.await
    }
}

// Example
async fn list_resources(req: RequestParams) -> Result<Response<Vec<u8>>, String> {
    log::debug!("{:?}", req);
    let res = Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/html; charset=utf-8")
        .body("Method allowed".as_bytes().to_vec())
        .unwrap();
    Ok(res)
}

pub struct AdminHttpApp {
    config: Admin,
    router: Router<HashMap<Method, AsyncHandlerWithArg<RequestParams>>>,
}

impl AdminHttpApp {
    pub fn new(config: Admin) -> Self {
        let mut this = Self {
            config: config.clone(),
            router: Router::new(),
        };

        let get_handler = AsyncHandlerWithArg::new(
            Method::GET,
            "/admin/{resource}/{id}".to_string(),
            list_resources,
        );

        this.route(get_handler);
        this
    }

    fn route(&mut self, handler: AsyncHandlerWithArg<RequestParams>) -> &mut Self {
        match self.router.at_mut(&handler.path) {
            Ok(routes) => {
                routes.value.insert(handler.method.clone(), handler);
            }
            Err(_) => {
                let mut handlers = HashMap::new();
                let path = handler.path.clone();
                handlers.insert(handler.method.clone(), handler);
                if let Err(err) = self.router.insert(path.clone(), handlers) {
                    panic!("Failed to insert path {}: {}", path, err);
                }
            }
        }
        self
    }
}

#[async_trait]
impl ServeHttp for AdminHttpApp {
    async fn response(&self, http_session: &mut ServerSession) -> Response<Vec<u8>> {
        http_session.set_keepalive(None);

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

                    match handler.call(params).await {
                        Ok(resp) => resp,
                        Err(e) => {
                            log::error!("Handler execution failed: {:?}", e);
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

pub fn admin_http_service(addr: &str) -> Service<AdminHttpApp> {
    let app = AdminHttpApp::new(Admin::default());
    let mut service = Service::new("Admin HTTP".to_string(), app);
    service.add_tcp(addr);
    service
}
