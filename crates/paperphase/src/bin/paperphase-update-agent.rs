//! Publisher tooling and an independent, narrowly configured app supervisor.
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use paperphase::update::{agent, package, store};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Parser)]
#[command(
    version,
    about = "Paperphase release signing and independent update supervisor"
)]
struct Args {
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    /// Supervise the app and activate verified updates after display acknowledgement.
    Run {
        #[arg(long)]
        installation: PathBuf,
    },
    /// Print private transaction status for SSH diagnostics (no credential contents).
    Status {
        #[arg(long)]
        installation: PathBuf,
    },
    /// Restore the previous release/data while the managed service is stopped.
    Recover {
        #[arg(long)]
        installation: PathBuf,
    },
    /// Sign a release package. Signing keys must remain outside the source/release.
    Pack {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        key: PathBuf,
        #[arg(long)]
        key_id: String,
        #[arg(long)]
        output: PathBuf,
    },
    /// Print only the public half of an existing publisher key.
    PublicKey {
        #[arg(long)]
        private_key: PathBuf,
    },
    /// Create a publisher key at an explicit private path; print only the public key.
    Keygen {
        #[arg(long)]
        private_key: PathBuf,
    },
}
fn main() -> Result<()> {
    match Args::parse().command {
        Action::Run { installation } => {
            let config = Arc::new(store::Installation::load(&installation)?);
            let listener = if config.status_port > 0 {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                Some(runtime.block_on(async {
                    let socket = tokio::net::TcpSocket::new_v4()?;
                    socket.set_reuseaddr(true)?;
                    socket.bind((std::net::Ipv4Addr::UNSPECIFIED, config.status_port).into())?;
                    socket.listen(32)?.into_std()
                })?)
            } else {
                None
            };
            let stopping = Arc::new(AtomicBool::new(false));
            let signal = Arc::clone(&stopping);
            std::thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("supervisor status runtime");
                runtime.block_on(async move {
                    let shutdown_signal = Arc::clone(&signal);
                    let shutdown = async move {
                        #[cfg(unix)]
                        {
                            let mut term = tokio::signal::unix::signal(
                                tokio::signal::unix::SignalKind::terminate(),
                            )
                            .expect("SIGTERM handler");
                            tokio::select! {_=tokio::signal::ctrl_c()=>{},_=term.recv()=>{}}
                        }
                        #[cfg(not(unix))]
                        {
                            let _ = tokio::signal::ctrl_c().await;
                        }
                        shutdown_signal.store(true, Ordering::SeqCst);
                    };
                    if let Some(listener) = listener {
                        let listener =
                            tokio::net::TcpListener::from_std(listener).expect("status listener");
                        if let Err(error) =
                            axum::serve(listener, paperphase::update::status::router(config))
                                .with_graceful_shutdown(shutdown)
                                .await
                        {
                            eprintln!("update status listener: {error}");
                        }
                    } else {
                        shutdown.await;
                    }
                    signal.store(true, Ordering::SeqCst);
                });
            });
            agent::run(&installation, stopping)
        }
        Action::Recover { installation } => agent::recover(&installation),
        Action::Status { installation } => {
            let config = store::Installation::load(&installation)?;
            println!("{}", serde_json::to_string_pretty(&config.job()?)?);
            Ok(())
        }
        Action::Pack {
            manifest,
            source,
            key,
            key_id,
            output,
        } => {
            let manifest: package::Manifest = store::read(&manifest)?;
            let private = read_private(&key)?;
            anyhow::ensure!(private.len() <= 4096, "invalid signing key size");
            package::write(&manifest, &key_id, &private, &source, &output)?;
            println!("Created {}", output.display());
            Ok(())
        }
        Action::PublicKey { private_key } => {
            use base64::Engine;
            use ring::signature::KeyPair;
            let private = read_private(&private_key)?;
            let key = ring::signature::Ed25519KeyPair::from_pkcs8(&private)
                .map_err(|_| anyhow::anyhow!("invalid Ed25519 signing key"))?;
            println!(
                "{}",
                base64::engine::general_purpose::STANDARD.encode(key.public_key().as_ref())
            );
            Ok(())
        }
        Action::Keygen { private_key } => {
            use base64::Engine;
            use ring::signature::KeyPair;
            use std::io::Write;
            let key =
                ring::signature::Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new())
                    .map_err(|_| anyhow::anyhow!("key generation failed"))?;
            let pair = ring::signature::Ed25519KeyPair::from_pkcs8(key.as_ref())
                .map_err(|_| anyhow::anyhow!("key creation failed"))?;
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options
                .open(private_key)
                .context("create a new private key outside the repository")?;
            file.write_all(key.as_ref())?;
            file.sync_all()?;
            println!(
                "{}",
                base64::engine::general_purpose::STANDARD.encode(pair.public_key().as_ref())
            );
            Ok(())
        }
    }
}

fn read_private(path: &std::path::Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(4097)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(bytes.len() <= 4096, "invalid signing key size");
    Ok(bytes)
}
