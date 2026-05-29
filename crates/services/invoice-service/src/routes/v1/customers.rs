use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use uuid::Uuid;

use lib_core::bmc::customer::CustomerBmc;
use lib_core::ctx::Ctx;
use lib_core::error::ErrorBody;
use lib_core::model::customer::{CustomerCreate, CustomerResponse};
use lib_core::{Error, Result};

use crate::routes::AppState;

#[utoipa::path(
    post,
    path = "/v1/customers",
    request_body = CustomerCreate,
    responses(
        (status = 201, description = "Customer created", body = CustomerResponse),
        (status = 400, description = "Validation error", body = ErrorBody),
        (status = 401, description = "Unauthorized", body = ErrorBody),
    ),
    security(("bearer_auth" = [])),
    tag = "Customers"
)]
pub async fn create_customer(
    State(state): State<AppState>,
    Extension(ctx): Extension<Ctx>,
    Json(req): Json<CustomerCreate>,
) -> Result<(StatusCode, Json<CustomerResponse>)> {
    if req.name.trim().is_empty() {
        return Err(Error::BadRequest("name is required".to_string()));
    }
    if req.email.trim().is_empty() {
        return Err(Error::BadRequest("email is required".to_string()));
    }
    // Basic email format validation
    if !req.email.contains('@') || !req.email.contains('.') {
        return Err(Error::BadRequest("Invalid email format".to_string()));
    }

    let customer = CustomerBmc::create(&state.db, &ctx, req).await?;

    Ok((StatusCode::CREATED, Json(customer.into())))
}

#[utoipa::path(
    get,
    path = "/v1/customers/{id}",
    params(("id" = Uuid, Path, description = "Customer UUID")),
    responses(
        (status = 200, description = "Customer found", body = CustomerResponse),
        (status = 404, description = "Customer not found", body = ErrorBody),
        (status = 401, description = "Unauthorized", body = ErrorBody),
    ),
    security(("bearer_auth" = [])),
    tag = "Customers"
)]
pub async fn get_customer(
    State(state): State<AppState>,
    Extension(ctx): Extension<Ctx>,
    Path(id): Path<Uuid>,
) -> Result<Json<CustomerResponse>> {
    let customer = CustomerBmc::get(&state.db, &ctx, id).await?;
    Ok(Json(customer.into()))
}

#[utoipa::path(
    get,
    path = "/v1/customers",
    responses(
        (status = 200, description = "List of customers", body = Vec<CustomerResponse>),
        (status = 401, description = "Unauthorized", body = ErrorBody),
    ),
    security(("bearer_auth" = [])),
    tag = "Customers"
)]
pub async fn list_customers(
    State(state): State<AppState>,
    Extension(ctx): Extension<Ctx>,
) -> Result<Json<Vec<CustomerResponse>>> {
    let customers = CustomerBmc::list(&state.db, &ctx).await?;
    let response: Vec<CustomerResponse> = customers.into_iter().map(Into::into).collect();
    Ok(Json(response))
}
