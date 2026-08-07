use std::process::ExitCode;

fn main() -> ExitCode {
    let bind = std::env::var("SYNC_HOST_BIND").unwrap_or_else(|_| "0.0.0.0:8232".to_string());
    let media_bind = std::env::var("SYNC_HOST_MEDIA").unwrap_or_else(|_| "127.0.0.1:8231".to_string());
    let interval = std::env::var("SYNC_HOST_INTERVAL")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(5 * 60);

    match no_pysop_lib::host::run_sync_host(&bind, &media_bind, interval) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sync_host error: {e}");
            ExitCode::FAILURE
        }
    }
}