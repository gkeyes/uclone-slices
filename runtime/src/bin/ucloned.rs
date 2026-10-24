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
use std::time::Duration;

use boot::{BOOT_ID_PATH, prepare_runtime_root, reconcile_once_per_boot, wait_for_user0_ready};
use rpc::{
    ConnectionLimit, MAX_CONCURRENT_CONNECTIONS, OPERATION_FAILED_RESPONSE, RequestHandler,
    serialize_transaction, spawn_connection,
};
use uclone_slices_runtime::launch::{LaunchGate, export_enrollment, validate_runtime_namespace};
use uclone_slices_runtime::layout::RUNTIME_ROOT;
use uclone_slices_runtime::{BINARY_BUILD_ID, handle_line, production_with_launch_gate};

/// system_server reconnects with up to 4 s backoff; this bounds the wait for a slow boot.
const SYSTEM_SUBSCRIPTION_WAIT: Duration = Duration::from_secs(30);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root =
        PathBuf::from(env::var("UCLONE_RUNTIME_ROOT").unwrap_or_else(|_| RUNTIME_ROOT.to_owned()));
    match env::args().nth(1).as_deref() {
        Some("--observe-foreground") => return foreground::observe_once(),
        Some("--export-enrollment") => {
            prepare_runtime_root(&root)?;
            export_enrollment(&root)?;
            return Ok(());
        }
        Some(_) => return Err("unknown daemon operation".into()),
        None => {}
    }
    validate_runtime_namespace()?;
    let socket = env::var("UCLONE_RUNTIME_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("runtime.sock"));
    let declared_build_id = env::var("UCLONE_BUILD_ID")?;
    if declared_build_id != BINARY_BUILD_ID {
        return Err("module configuration does not match the executable build".into());
    }
    let build_id = BINARY_BUILD_ID.to_owned();
    prepare_runtime_root(&root)?;
    let boot_id = fs::read_to_string(BOOT_ID_PATH)?;
    let transaction_lock = Arc::new(Mutex::new(()));
    let gate = LaunchGate::new(&root);
    let prepare_gate = Arc::clone(&gate);
    let prepare_root = root.clone();
    let prepare_lock = Arc::clone(&transaction_lock);
    let prepare_build = build_id.clone();
    gate.start(Arc::new(move |package, ticket, signing| {
        let _transaction = prepare_lock
            .lock()
            .map_err(|_| "transaction lock failed".to_owned())?;
        if !prepare_gate.ticket_live(ticket) {
            return Err("launch was cancelled".to_owned());
        }
        let mut runtime = production_with_launch_gate(
            &prepare_root,
            prepare_build.clone(),
            Arc::clone(&prepare_gate),
        )
        .map_err(|error| error.to_string())?;
        // An updated App, or one whose /data device was renumbered by a reboot, is refused
        // at specialization until re-bound. The Manager's signer reading proves it is the
        // same App, so the gate re-binds it here instead of requiring the Manager be opened.
        let rebound = match signing {
            Some(signing) => match prepare_gate.installed_apk_changed(package) {
                Ok(true) => runtime
                    .rebind_installed(package, signing)
                    .map_err(|error| error.to_string()),
                Ok(false) => Ok(()),
                Err(error) => Err(error.to_string()),
            },
            None => Ok(()),
        };
        let prepared = rebound
            .and_then(|()| {
                runtime
                    .prepare_default_account(package, || prepare_gate.ticket_live(ticket))
                    .map_err(|error| error.to_string())
            })
            .and_then(|_| {
                prepare_gate
                    .permit_prepared(package, ticket)
                    .map_err(|error| error.to_string())
            });
        prepare_gate
            .finish_transaction()
            .map_err(|error| error.to_string())?;
        prepared
    }))?;
    // The launch name is bound before waiting for unlock, which leaves an App no window to
    // claim it first, and lets system_server subscribe while the user is still locked.
    wait_for_user0_ready();
    // Boot convergence force-stops and may relaunch Apps, which needs the system
    // subscription. Waiting is bounded; without it convergence still runs and its marker
    // stays unwritten on failure, so the next daemon start in this boot retries it.
    match gate.await_system(SYSTEM_SUBSCRIPTION_WAIT) {
        Ok(true) => {}
        Ok(false) => eprintln!("ucloned boot reconcile: system support not subscribed yet"),
        Err(error) => eprintln!("ucloned boot reconcile: {error}"),
    }
    // A failed boot convergence is logged and retried on the next start in this boot (its
    // marker is not written). The socket still opens: every request re-validates its own
    // package, and a package whose state cannot be read keeps failing closed on its own.
    let boot_result = (|| {
        let _transaction = transaction_lock
            .lock()
            .map_err(|_| "transaction lock failed")?;
        production_with_launch_gate(&root, build_id.clone(), Arc::clone(&gate))
            .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
            .and_then(|mut boot_runtime| {
                let result = reconcile_once_per_boot(&root, boot_id.trim(), || {
                    boot_runtime.reconcile_boot()
                });
                gate.finish_transaction()?;
                result
            })
    })();
    if let Err(error) = boot_result {
        eprintln!("ucloned boot reconcile failed; serving requests anyway: {error}");
    }
    remove_stale_socket(&socket)?;
    let listener = UnixListener::bind(&socket)?;
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))?;
    let handler_root = root;
    let handler_build_id = build_id;
    let handler: Arc<RequestHandler> = Arc::new(move |line| {
        serialize_transaction(&transaction_lock, || {
            match production_with_launch_gate(
                &handler_root,
                handler_build_id.clone(),
                Arc::clone(&gate),
            ) {
                Ok(mut runtime) => {
                    let response = handle_line(&mut runtime, &handler_build_id, line);
                    if let Err(error) = gate.finish_transaction() {
                        eprintln!("launch policy refresh failed: {error}");
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
