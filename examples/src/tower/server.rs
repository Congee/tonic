use hyper::Body;
use std::{
    future::Future,
    task::{Context, Poll},
    time::Duration,
};
use tonic::{body::BoxBody, transport::Server, Request, Response, Status};
use tower::{Layer, Service};

use glommio;
use hello_world::greeter_server::{Greeter, GreeterServer};
use hello_world::{HelloReply, HelloRequest};

#[derive(Copy, Clone)]
struct GlommioExec;

impl<Fut> hyper::rt::Executor<Fut> for GlommioExec
where
    Fut: Future + Send + 'static,
    Fut::Output: Send + 'static,
{
    fn execute(&self, fut: Fut) {
        glommio::spawn_local(fut).detach();
    }
}

pub mod hello_world {
    tonic::include_proto!("helloworld");
}

#[derive(Default)]
pub struct MyGreeter {}

#[tonic::async_trait]
impl Greeter for MyGreeter {
    async fn say_hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloReply>, Status> {
        println!("Got a request from {:?}", request.remote_addr());

        let reply = hello_world::HelloReply {
            message: format!("Hello {}!", request.into_inner().name),
        };
        Ok(Response::new(reply))
    }
}

struct Gter;

#[tonic::async_trait]
impl Greeter for Gter {
    async fn say_hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloReply>, Status> {
        println!("Got a request from {:?}", request.remote_addr());

        let reply = hello_world::HelloReply {
            message: format!("Hello {}!", request.into_inner().name),
        };
        Ok(Response::new(reply))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse().unwrap();
    let greeter = MyGreeter::default();

    println!("GreeterServer listening on {}", addr);

    let svc = GreeterServer::new(greeter);

    // The stack of middleware that our service will be wrapped in
    let layer = tower::ServiceBuilder::new()
        // Apply middleware from tower
        .timeout(Duration::from_secs(30))
        // Apply our own middleware
        .layer(MyMiddlewareLayer::default())
        // Interceptors can be also be applied as middleware
        .layer(tonic::service::interceptor(intercept))
        .into_inner();

    Server::builder()
        .executor(GlommioExec)
        // Wrap all services in the middleware stack
        .layer(layer)
        .add_service(svc)
        .serve(addr)
        .await?;

    Ok(())
}

// An interceptor function.
fn intercept(req: Request<()>) -> Result<Request<()>, Status> {
    Ok(req)
}

#[derive(Debug, Clone, Default)]
struct MyMiddlewareLayer;

impl<S> Layer<S> for MyMiddlewareLayer {
    type Service = MyMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        MyMiddleware { inner: service }
    }
}

#[derive(Debug, Clone)]
struct MyMiddleware<S> {
    inner: S,
}

impl<S> Service<hyper::Request<Body>> for MyMiddleware<S>
where
    S: Service<hyper::Request<Body>, Response = hyper::Response<BoxBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = futures::future::BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: hyper::Request<Body>) -> Self::Future {
        // This is necessary because tonic internally uses `tower::buffer::Buffer`.
        // See https://github.com/tower-rs/tower/issues/547#issuecomment-767629149
        // for details on why this is necessary
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);

        Box::pin(async move {
            // Do extra async work here...
            let response = inner.call(req).await?;

            Ok(response)
        })
    }
}
