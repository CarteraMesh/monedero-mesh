mod settlement;

use {
    crate::{
        actors::actor_spawn,
        rpc::{
            Controller,
            Metadata,
            RelayProtocol,
            ResponseParamsError,
            RpcResponsePayload,
            SdkErrors,
            SessionProposeRequest,
            SessionSettleRequest,
        },
        session::{Category, PendingSession},
        spawn_task,
        wallet::settlement::WalletSettlementActor,
        PairingManager,
        ProposeFuture,
        Result,
        SessionHandler,
        WalletSettlementHandler,
    },
    monedero_domain::{Pairing, SessionSettled},
    std::{
        fmt::{Debug, Display, Formatter},
        str::FromStr,
        sync::Arc,
    },
    tracing::{error, warn},
    xtra::prelude::*,
};

#[derive(Clone, xtra::Actor)]
pub struct Wallet {
    manager: PairingManager,
    pending: Arc<PendingSession>,
    settlement_handler: Address<WalletSettlementActor>,
    metadata: Metadata,
}

impl Display for Wallet {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}]", self.metadata.name)
    }
}

impl Debug for Wallet {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "[wallet:{}]", self.metadata.name)
    }
}

impl Wallet {
    #[tracing::instrument(skip(request), level = "info")]
    async fn send_settlement(
        &self,
        request: SessionProposeRequest,
        public_key: String,
    ) -> Result<()> {
        let session_topic = self
            .manager
            .register_dapp_pk(request.proposer.clone())
            .await?;
        let namespaces = self.settlement_handler.send(request).await??;
        let now = chrono::Utc::now();
        let future = now + chrono::Duration::hours(24);
        let session_settlement = SessionSettleRequest {
            relay: RelayProtocol::default(),
            controller: Controller {
                public_key,
                metadata: self.metadata.clone(),
            },
            namespaces: namespaces.clone(),
            expiry: future.timestamp(),
        };
        self.pending
            .settled(
                &self.manager,
                SessionSettled {
                    topic: session_topic,
                    namespaces,
                    expiry: session_settlement.expiry,
                },
                Category::Wallet,
                Some(session_settlement),
            )
            .await?;
        Ok(())
    }
}

async fn send_settlement(wallet: Wallet, request: SessionProposeRequest, public_key: String) {
    if let Err(e) = wallet.send_settlement(request, public_key).await {
        warn!("failed to create ClientSession: '{e}'");
    }
}

impl Handler<SessionProposeRequest> for Wallet {
    type Return = RpcResponsePayload;

    async fn handle(
        &mut self,
        message: SessionProposeRequest,
        _ctx: &mut Context<Self>,
    ) -> Self::Return {
        let pk = self.manager.pair_key();
        if pk.is_none() {
            error!("no pairing key!");
            return RpcResponsePayload::Error(ResponseParamsError::SessionPropose(
                SdkErrors::UserRejected.into(),
            ));
        }
        let pk = pk.unwrap();
        if let Ok((accepted, response)) = self
            .settlement_handler
            .send(SessionProposePublicKey(String::from(&pk), message.clone()))
            .await
        {
            if accepted {
                let wallet = self.clone();
                spawn_task(async move { send_settlement(wallet, message, pk).await });
            }
            return response;
        }
        error!("failed sending verify to actor");
        RpcResponsePayload::Error(ResponseParamsError::SessionPropose(
            SdkErrors::UserRejected.into(),
        ))
    }
}

struct SessionProposePublicKey(pub String, pub SessionProposeRequest);

impl Wallet {
    pub async fn new<T: WalletSettlementHandler>(
        manager: PairingManager,
        handler: T,
    ) -> Result<Self> {
        let metadata = Metadata {
            name: "mock wallet".to_string(),
            description: "mocked wallet".to_string(),
            url: "https://example.com".to_string(),
            icons: vec![],
            verify_url: None,
            redirect: None,
        };
        let settlement_handler = actor_spawn(WalletSettlementActor::new(handler));

        let me = Self {
            manager,
            pending: Arc::new(PendingSession::new()),
            metadata,
            settlement_handler,
        };
        me.manager.actors().proposal().send(me.clone()).await?;
        Ok(me)
    }

    #[tracing::instrument(skip(handlers), level = "info")]
    pub async fn pair<T: SessionHandler>(
        &self,
        uri: String,
        handlers: T,
    ) -> Result<(Pairing, ProposeFuture)> {
        let pairing = Pairing::from_str(&uri)?;
        let rx = self.pending.add(pairing.topic.clone(), handlers);
        self.manager.set_pairing(pairing.clone()).await?;
        Ok((pairing, ProposeFuture::new(rx)))
    }
}
