use std::future::{ready, Ready};
use actix_web::{
    dev::{self, Service, ServiceRequest, ServiceResponse, Transform},
    Error,
};
use futures_util::future::LocalBoxFuture;

// Define path logging middleware
pub struct PathLogger;

impl<S, B> Transform<S, ServiceRequest> for PathLogger
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = PathLoggerMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(PathLoggerMiddleware { service }))
    }
}

pub struct PathLoggerMiddleware<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for PathLoggerMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    dev::forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        println!("=== REQUEST DEBUG INFO ===");
        println!("Path: {}", req.path());
        println!("Method: {}", req.method());
        println!("Path parameters: {:?}", req.match_info());
        println!("=========================");

        let fut = self.service.call(req);

        Box::pin(async move {
            let res = fut.await?;
            println!("=== RESPONSE DEBUG INFO ===");
            println!("Response status: {}", res.status());
            println!("===========================");
            Ok(res)
        })
    }
}