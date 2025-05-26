use actix_web::{
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpMessage,
};
use futures_util::future::{ok, LocalBoxFuture, Ready};
use futures_util::FutureExt;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::Deserialize;

use crate::utils::signature::verify_hmac;

/// Minimal subset we care about for JWT.
#[derive(Debug, Deserialize)]
struct StdClaims {
    sub: Option<String>,
}

pub struct Auth;

impl<S> Transform<S, ServiceRequest> for Auth
where
    S: Service<ServiceRequest, Response = ServiceResponse, Error = Error> + 'static,
{
    type Response  = ServiceResponse;
    type Error     = Error;
    type InitError = ();
    type Transform = AuthMw<S>;
    type Future    = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, srv: S) -> Self::Future {
        ok(AuthMw { inner: srv })
    }
}

pub struct AuthMw<S> {
    inner: S,
}

impl<S> Service<ServiceRequest> for AuthMw<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse, Error = Error> + 'static,
{
    type Response = ServiceResponse;
    type Error    = Error;
    type Future   = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &self,
        ctx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(ctx)
    }

    fn call(&self, mut req: ServiceRequest) -> Self::Future {
        let is_get = req.method() == actix_web::http::Method::GET;

        let fut = async move {
            // --- 1. Buffer body if non‑GET -------------------------------------
            if !is_get {
                use actix_web::web::BytesMut;
                use futures_util::StreamExt;

                let mut payload = req.take_payload();
                let mut body = BytesMut::new();

                while let Some(chunk) = payload.next().await {
                    let chunk = chunk.map_err(Error::from)?;
                    body.extend_from_slice(&chunk);
                }
                req.extensions_mut().insert(body.to_vec());
            }

            // --- 2. Extract JWT -------------------------------------------------
            let token_hdr = req
                .headers()
                .get("Authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.strip_prefix("Bearer "))
                .map(str::to_owned);

            let jwt_secret = std::env::var("DISCORD_JWT_SECRET").unwrap_or_default();
            let jwt_result = token_hdr.as_deref().map(|tok| {
                decode::<StdClaims>(
                    tok,
                    &DecodingKey::from_secret(jwt_secret.as_bytes()),
                    &Validation::new(Algorithm::HS256),
                )
            });

            let jwt_ok = jwt_result.as_ref().map(|r| r.is_ok()).unwrap_or(false);

            // --- 3. Verify HMAC -------------------------------------------------
            let hmac_ok = verify_hmac(&req);

            // --- 4. Inject user ID if valid and forward -------------------------
            if jwt_ok || hmac_ok {
                if let Some(Ok(data)) = jwt_result {
                    if let Some(uid) = data.claims.sub {
                        req.extensions_mut().insert(uid);
                    }
                }
                self.inner.call(req).await
            } else {
                Err(actix_web::error::ErrorUnauthorized("auth failed"))
            }
        };

        fut.boxed_local()
    }
}
