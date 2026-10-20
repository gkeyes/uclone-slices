#[path = "ucloned/boot.rs"]
mod boot;
#[path = "ucloned/foreground.rs"]
mod foreground;
#[path = "ucloned/rpc.rs"]
mod rpc;

use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use boot::{BOOT_ID_PATH, prepare_runtime_root, reconcile_once_per_boot, wait_for_user0_ready};
use rpc::{
    ConnectionLimit, MAX_CONCURRENT_CONNECTIONS, OPERATION_FAILED_RESPONSE, RequestHandler,
    serialize_transaction, spawn_connection,
};
use uclone_slices_runtime::layout::RUNTIME_ROOT;
use uclone_slices_runtime::{BINARY_BUILD_ID, handle_line, production};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::args().nth(1).as_deref() == Some("--observe-foreground") {
        return foreground::observe_once();
    }
    let root =
        PathBuf::from(env::var("UCLONE_RUNTIME_ROOT").unwrap_or_else(|_| RUNTIME_ROOT.to_owned()));
    let socket = env::var("UCLONE_RUNTIME_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("runtime.sock"));
    let declared_build_id = env::var("UCLONE_BUILD_ID")?;
    if declared_build_id != BINARY_BUILD_ID {
        return Err("module configuration does not match the executable build".into());
    }
    let build_id = BINARY_BUILD_ID.to_owned();
    prepare_runtime_root(&root)?;
    wait_for_user0_ready();
    let boot_id = fs::read_to_string(BOOT_ID_PATH)?;
    // A failed boot convergence is logged and retried on the next start in this boot (its
    // marker is not written). The socket still opens: every request re-validates its own
    // package, and a package whose state cannot be read keeps failing closed on its own.
    let boot_result = production(&root, build_id.clone())
        .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
        .and_then(|mut boot_runtime| {
            reconcile_once_per_boot(&root, boot_id.trim(), || boot_runtime.reconcile_boot())
        });
    if let Err(error) = boot_result {
        eprintln!("ucloned boot reconcile failed; serving requests anyway: {error}");
    }
    remove_stale_socket(&socket)?;
    let listener = UnixListener::bind(&socket)?;
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))?;
    let transaction_lock = Arc::new(Mutex::new(()));
    let sessions = Arc::new(Mutex::new(foreground::Sessions::default()));
    foreground::spawn(
        root.clone(),
        build_id.clone(),
        Arc::clone(&transaction_lock),
        Arc::clone(&sessions),
    );
    let handler_root = root;
    let handler_build_id = build_id;
    let handler: Arc<RequestHandler> = Arc::new(move |line| {
        serialize_transaction(&transaction_lock, || {
            match production(&handler_root, handler_build_id.clone()) {
                Ok(mut runtime) => {
                    let response = handle_line(&mut runtime, &handler_build_id, line);
                    if let Ok(mut sessions) = sessions.lock() {
                        sessions.completed_request(line, &response);
                    }
                    response
                }
                Err(error) => {
                    eprintln!("ucloned Runtime unavailable: {error}");
                    OPERATION_FAILED_RESPONSE.to_owned()
                }
            }
        })
    });
    let connections = ConnectionLimit::new(MAX_CONCURRENT_CONNECTIONS);
    loop {
        // Waiting here (not after accept) leaves extra clients in the listen backlog.
        let permit = connections.acquire();
        match listener.accept() {
            Ok((stream, _address)) => drop(spawn_connection(stream, Arc::clone(&handler), permit)),
            Err(error) => eprintln!("ucloned accept failed: {error}"),
        }
    }
}

fn remove_stale_socket(path: &Path) -> Result<(), std::io::Error> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
