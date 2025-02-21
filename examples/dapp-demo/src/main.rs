use {
    copypasta::{ClipboardContext, ClipboardProvider},
    monedero_mesh::{
        self,
        domain::{
            namespaces::{ChainId, ChainType, Chains, Method, NamespaceName, SolanaMethod},
            Pairing,
            ProjectId,
        },
        init_tracing,
        rpc::{RequestMethod, RequestParams, SessionRequestRequest},
        ClientSession,
        Dapp,
        KvStorage,
        Metadata,
        NoopSessionHandler,
        ReownBuilder,
    },
    serde_json::json,
    std::time::Duration,
    tokio::{select, signal},
    tracing::{error, info},
};

async fn propose(dapp: &Dapp) -> anyhow::Result<(Pairing, ClientSession)> {
    let chains = Chains::from([
        ChainId::Solana(ChainType::Dev),
        ChainId::EIP155(alloy_chains::Chain::sepolia()),
    ]);
    info!("purposing chains {chains}");
    let (p, rx, restored) = dapp.propose(NoopSessionHandler, &chains).await?;
    let mut ctx = ClipboardContext::new().expect("Failed to open clipboard");
    ctx.set_contents(p.to_string())
        .expect("Failed to set clipboard");
    if !restored {
        // qr2term::print_qr(&p.to_string())?;
        eprintln!("\n\n{p}\n\n");
    }
    let session = rx.await?;
    Ok((p, session))
}

async fn pair_ping(dapp: Dapp) {
    loop {
        info!("sending pair ping");
        if let Err(e) = dapp.pair_ping().await {
            error!("pair ping failed! {e}");
        }
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}

async fn sign_message(session: ClientSession) {
    if !session.namespaces().0.contains_key(&NamespaceName::Solana) {
        return;
    }
    let sol_namespace = session.namespaces().0.get(&NamespaceName::Solana).unwrap();
    for a in &sol_namespace.accounts.0 {
        let addr = &a.address;
        info!("found solana address {addr}");
        let params: RequestParams = RequestParams::SessionRequest(SessionRequestRequest {
            request: RequestMethod {
                method: Method::Solana(SolanaMethod::SignMessage),
                params: json!({
                    "message": "37u9WtQpcm6ULa3VtWDFAWoQc1hUvybPrA3dtx99tgHvvcE7pKRZjuGmn7VX2tC3JmYDYGG7",
                    "pubkey": addr,
                }),
                expiry: None,
            },
            chain_id: a.chain.clone(),
        });
        info!(
            "signing a personal message\n{}",
            serde_json::to_string_pretty(&params).unwrap()
        );
        match session.publish_request::<serde_json::Value>(params).await {
            Err(e) => {
                error!("failed to publish message! {e}");
            }
            Ok(r) => {
                info!(
                    "got back signature response\n{:?}",
                    serde_json::to_string_pretty(&r).unwrap()
                );
            }
        };
    }
}

async fn do_dapp_stuff(dapp: Dapp) {
    info!("Running dapp - hit control-c to terminate");
    let session = match propose(&dapp).await {
        Err(e) => {
            error!("failed to get session! {e}");
            return;
        }
        Ok((_, s)) => s,
    };
    info!("settled {:#?}", session.namespaces());
    let pinger = dapp.clone();
    tokio::spawn(pair_ping(pinger));
    tokio::spawn(sign_message(session.clone()));
    loop {
        info!("sending session ping");
        if let Err(e) = session.ping().await {
            error!("session ping failed! {e}");
        }
        tokio::time::sleep(Duration::from_secs(15)).await;
    }
}

#[allow(clippy::redundant_pub_crate)]
async fn dapp_test() -> anyhow::Result<()> {
    let p = ProjectId::from("987f2292c12194ae69ddb6c52ceb1d62");
    let store = KvStorage::file(None)?;
    let pairing_mgr = ReownBuilder::new(p).store(store).build().await?;
    let dapp = Dapp::new(pairing_mgr.clone(), Metadata {
        name: "monedero-mesh".to_string(),
        description: "reown but for rust".to_string(),
        url: String::from(monedero_mesh::AUTH_URL),
        icons: vec![],
        verify_url: None,
        redirect: None,
    })
    .await?;
    tokio::spawn(do_dapp_stuff(dapp));

    let ctrl_c = signal::ctrl_c();
    let mut term = signal::unix::signal(signal::unix::SignalKind::terminate())?;

    select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, shutting down...");
        }
        _ = term.recv() => {
            info!("Received SIGTERM, shutting down...");
        }
    }
    pairing_mgr.shutdown().await?;
    Ok(())
}

#[cfg(not(target_family = "wasm"))]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    dapp_test().await
}
