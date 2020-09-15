pub use lighthouse_metrics::*;

lazy_static::lazy_static! {
    pub static ref HTTP_API_PATHS_TOTAL: Result<IntCounterVec> = try_create_int_counter_vec(
        "http_api_paths_total",
        "Count of HTTP requests received",
        &["path"]
    );
    pub static ref HTTP_API_STATUS_CODES_TOTAL: Result<IntCounterVec> = try_create_int_counter_vec(
        "http_api_status_codes_total",
        "Count of HTTP status codes returned",
        &["status"]
    );
    pub static ref HTTP_API_PATHS_TIMES_TOTAL: Result<HistogramVec> = try_create_histogram_vec(
        "http_api_paths_times_total",
        "Duration to process HTTP requests per path",
        &["path"]
    );
}
