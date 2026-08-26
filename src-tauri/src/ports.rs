use std::net::TcpListener;

/// Ask the OS for a free TCP port on localhost by binding to port 0 and
/// reading back whatever it assigned, then releasing it immediately.
///
/// There's a small window between releasing the port here and something else
/// grabbing it, but for a single-user local desktop app that risk is
/// negligible and this keeps the code simple.
pub fn free_local_port() -> anyhow::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}
