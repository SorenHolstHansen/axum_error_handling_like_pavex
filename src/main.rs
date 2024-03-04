use axum::{
    error_handling::HandleError,
    extract::{FromRequest, FromRequestParts, Request},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get_service,
    Router,
};
use std::{fmt::Display, future::Future, pin::Pin, time::Duration};
use tower_http::trace::TraceLayer;
use tracing::Span;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

trait CustomHandler<T, S, Err>: Send + Sized + 'static {
    type Future: Future<Output = Result<Response, Err>> + Send + 'static;

    fn call_me(self, req: Request, state: S) -> Self::Future;
}

macro_rules! all_the_tuples {
    ($name:ident) => {
        $name!([], T1);
        $name!([T1], T2);
        $name!([T1, T2], T3);
        $name!([T1, T2, T3], T4);
        $name!([T1, T2, T3, T4], T5);
        $name!([T1, T2, T3, T4, T5], T6);
        $name!([T1, T2, T3, T4, T5, T6], T7);
        $name!([T1, T2, T3, T4, T5, T6, T7], T8);
        $name!([T1, T2, T3, T4, T5, T6, T7, T8], T9);
        $name!([T1, T2, T3, T4, T5, T6, T7, T8, T9], T10);
        $name!([T1, T2, T3, T4, T5, T6, T7, T8, T9, T10], T11);
        $name!([T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11], T12);
        $name!([T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12], T13);
        $name!(
            [T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13],
            T14
        );
        $name!(
            [T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14],
            T15
        );
        $name!(
            [T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15],
            T16
        );
    };
}

macro_rules! impl_handler {
    (
        [$($ty:ident),*], $last:ident
    ) => {
        #[allow(non_snake_case, unused_mut)]
        impl<F, Fut, Res, S, Err, M, $($ty,)* $last> CustomHandler<(M, $($ty,)* $last,), S, Err> for F
        where
            F: FnOnce($($ty,)* $last,) -> Fut + Send + 'static,
            Fut: Future<Output = Result<Res, Err>> + Send,
            Res: IntoResponse,
            Err: Send + Sync + std::error::Error + 'static,
            S: Send + Sync + 'static,
            $( $ty: FromRequestParts<S> + Send, )*
            $last: FromRequest<S, M> + Send,
        {
            type Future = Pin<Box<dyn Future<Output = Result<Response, Err>> + Send>>;

            fn call_me(self, req: Request, state: S) -> Self::Future {
                Box::pin(async move {
                    let (mut parts, body) = req.into_parts();
                    let state = &state;

                    $(
                        let $ty = match $ty::from_request_parts(&mut parts, state).await {
                            Ok(value) => value,
                            Err(rejection) => return Ok(rejection.into_response()),
                        };
                    )*

                    let req = Request::from_parts(parts, body);

                    let $last = match $last::from_request(req, state).await {
                        Ok(value) => value,
                        Err(rejection) => return Ok(rejection.into_response()),
                    };

                    let res = self($($ty,)* $last,).await.map_err(|e| {
                        tracing::error!(
                            error.message = %e,
                            error.details = ?e,
                            "An error occurred during request handling"
                        );
                        e
                    })?;

                    Ok(res.into_response())
                })
            }
        }
    };
}

all_the_tuples!(impl_handler);

macro_rules! handled_service {
    ($f:ident, $ef:ident) => {
        HandleError::new(
            tower::service_fn(move |req: Request| async { Ok($f.call_me(req, ()).await?) }),
            $ef,
        )
    };
}

#[derive(Debug)]
struct MyErr;

impl Display for MyErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Oh no!")
    }
}

impl std::error::Error for MyErr {}

/// Can use arbitrary extractors here
async fn handler(_header: HeaderMap, _req: Request) -> Result<StatusCode, MyErr> {
    Err(MyErr)
}

/// Can also do arbitrary extractors here
async fn handle_error(_header: HeaderMap, _err: MyErr) -> StatusCode {
    StatusCode::INTERNAL_SERVER_ERROR
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "axum_error_handling_like_pavex=debug,tower_http=debug,axum::rejection=trace".into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let app = Router::new()
        .route("/", get_service(handled_service!(handler, handle_error)))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request<_>| {
                    let span = tracing::info_span!(
                        "HTTP request",
                        http.request.method = request.method().as_str(),
                        // ...
                        http.response.status_code = tracing::field::Empty,
                        error.message = tracing::field::Empty,
                        error.details = tracing::field::Empty,
                    );

                    span
                })
                .on_response(
                    |response: &Response, _latency: Duration, root_span: &Span| {
                        root_span.record("http.response.status_code", response.status().as_u16());
                    },
                )
                // Opt-out of on_failure, since we are handling it elsewhere
                .on_failure(()),
        );

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8888").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
