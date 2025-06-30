//-------------------------------------------------------------
// src/middleware/metrics.rs
//-------------------------------------------------------------
use std::future::{ready, Future, Ready};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::Error;
use metrics::{histogram, increment_counter};

pub struct Metrics;

impl<S, B> Transform<S, ServiceRequest> for Metrics
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = MetricsSvc<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, srv: S) -> Self::Future {
        ready(Ok(MetricsSvc { inner: srv }))
    }
}

pub struct MetricsSvc<S> {
    inner: S,
}

impl<S, B> Service<ServiceRequest> for MetricsSvc<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&self, ctx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(ctx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        // ---------------------------  before  ---------------------------
        let start = Instant::now();
        let method = req.method().as_str().to_string();
        let path   = req.path().to_string();

        // Leak the strings so we can hand `'static` references to the macro
        let method_leaked : &'static str = Box::leak(method.into_boxed_str());
        let path_leaked   : &'static str = Box::leak(path.into_boxed_str());

        // ---------------------------  call next  ------------------------
        let fut = self.inner.call(req);

        Box::pin(async move {
            let res = fut.await?;
            let latency = start.elapsed().as_secs_f64() * 1_000.0; // → ms
            let status_string = res.status().as_u16().to_string();
            let status_leaked : &'static str = Box::leak(status_string.into_boxed_str());

            increment_counter!(
                "http_requests_total",
                "method" => method_leaked,
                "path"   => path_leaked,
                "status" => status_leaked,
            );

            histogram!(
                "http_latency_ms",
                latency,
                "method" => method_leaked,
                "path"   => path_leaked,
            );

            Ok(res)
        })
    }
}
