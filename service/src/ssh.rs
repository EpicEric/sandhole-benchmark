use std::sync::Arc;

use color_eyre::{Result, eyre::WrapErr, eyre::eyre};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto::Builder,
};
use russh::{
    Channel, ChannelId, ChannelMsg, Disconnect,
    client::{self, Config, Handle, Msg, Session, connect_stream},
    keys::{HashAlg, PrivateKey, PrivateKeyWithHashAlg, ssh_key},
};
use tokio::io::{AsyncWriteExt, stderr, stdout};
use tracing::{debug, info, instrument, trace, warn};

use crate::RouterService;

/* Russh session and client */

/// User-implemented session type as a helper for interfacing with the SSH protocol.
pub(crate) struct TcpForwardSession(Handle<Client>);

/// User-implemented session type as a helper for interfacing with the SSH protocol.
impl TcpForwardSession {
    #[instrument(level = "debug")]
    pub(crate) async fn connect_key(
        host: &str,
        port: u16,
        login_name: &str,
        key: Arc<PrivateKey>,
        config: Arc<Config>,
        client_service: RouterService,
    ) -> Result<Self> {
        debug!("TcpForwardSession connecting...");
        let socket = tokio::net::TcpStream::connect((host, port)).await?;
        if let Err(err) = socket.set_nodelay(true) {
            debug!("Failed to set nodelay: {err}");
        }
        match connect_stream(
            Arc::clone(&config),
            socket,
            Client {
                server_fingerprint: None,
                service: client_service,
            },
        )
        .await
        {
            Ok(mut session) => {
                if session
                    .authenticate_publickey(
                        login_name,
                        PrivateKeyWithHashAlg::new(
                            key,
                            session
                                .best_supported_rsa_hash()
                                .await?
                                .flatten()
                                .or(Some(HashAlg::Sha256)),
                        ),
                    )
                    .await
                    .wrap_err_with(|| "Error while authenticating with key.")?
                    .success()
                {
                    debug!("Key authentication succeeded!");
                    Ok(Self(session))
                } else {
                    Err(eyre!("Key authentication failed."))
                }
            }
            Err(err) => Err(err).wrap_err_with(|| "Unable to connect to remote host."),
        }
    }

    /// Sends a port forwarding request and opens a session to receive miscellaneous data.
    /// The function yields when the session is broken (for example, if the connection was lost).
    #[instrument(level = "debug", skip(self))]
    pub(crate) async fn start_forwarding(&mut self) -> Result<u32> {
        let session = &mut self.0;
        let mut channel = session
            .channel_open_session()
            .await
            .wrap_err_with(|| "channel_open_session error.")?;
        debug!("Created open session channel.");
        session
            .tcpip_forward("measure", 80)
            .await
            .wrap_err_with(|| "tcpip_forward error.")?;
        debug!("Requested tcpip_forward session.");
        // let mut stdin = stdin();
        let mut stdout = stdout();
        let mut stderr = stderr();
        let code = loop {
            let Some(msg) = channel.wait().await else {
                return Err(eyre!("Unexpected end of channel."));
            };
            trace!("Got a message through initial session!");
            match msg {
                ChannelMsg::Data { ref data } => {
                    stdout.write_all(data).await?;
                    stdout.flush().await?;
                }
                ChannelMsg::ExtendedData { ref data, ext: 1 } => {
                    stderr.write_all(data).await?;
                    stderr.flush().await?;
                }
                ChannelMsg::Success => (),
                ChannelMsg::Close => break 0,
                ChannelMsg::ExitStatus { exit_status } => {
                    debug!("Exited with code {exit_status}");
                    channel
                        .eof()
                        .await
                        .wrap_err_with(|| "Unable to close connection.")?;
                    break exit_status;
                }
                msg => return Err(eyre!("Unknown message type {:?}.", msg)),
            }
        };
        Ok(code)
    }

    pub async fn close(&mut self) -> Result<()> {
        self.0
            .disconnect(Disconnect::ByApplication, "", "English")
            .await?;
        Ok(())
    }
}

/// Our SSH client implementing the `Handler` callbacks for the functions we need to use.
struct Client {
    server_fingerprint: Option<String>,
    service: RouterService,
}

impl client::Handler for Client {
    type Error = color_eyre::eyre::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        if let Some(server_fingerprint) = &self.server_fingerprint {
            Ok(&server_public_key.fingerprint(HashAlg::Sha256).to_string() == server_fingerprint)
        } else {
            Ok(true)
        }
    }

    #[instrument(level = "debug", skip(self))]
    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<Msg>,
        connected_address: &str,
        connected_port: u32,
        originator_address: &str,
        originator_port: u32,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let hyper_service = self.service.clone();
        tokio::spawn(async move {
            Builder::new(TokioExecutor::new())
                .serve_connection_with_upgrades(TokioIo::new(channel.into_stream()), hyper_service)
                .await
                .expect("Invalid request");
        });
        Ok(())
    }

    async fn auth_banner(
        &mut self,
        banner: &str,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        debug!("Received auth banner.");
        let mut stdout = stdout();
        stdout.write_all(banner.as_bytes()).await?;
        stdout.flush().await?;
        Ok(())
    }

    async fn exit_status(
        &mut self,
        channel: ChannelId,
        exit_status: u32,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        debug!(channel = ?channel, "exit_status");
        if exit_status == 0 {
            info!("Remote exited with status {}.", exit_status);
        } else {
            warn!("Remote exited with status {}.", exit_status);
        }
        Ok(())
    }

    async fn channel_open_confirmation(
        &mut self,
        channel: ChannelId,
        max_packet_size: u32,
        window_size: u32,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        debug!(channel = ?channel, max_packet_size, window_size, "channel_open_confirmation");
        Ok(())
    }

    async fn channel_success(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        debug!(channel = ?channel, "channel_success");
        Ok(())
    }
}
