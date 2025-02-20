use {
    crate::{
        rpc::{Proposer, SessionProposeResponse},
        PairingManager,
        Result,
    },
    monedero_domain::{SessionTopic, Topic},
    tracing::info,
};

impl PairingManager {
    pub(super) async fn restore_saved_pairing(&self) -> Result<()> {
        if let Some(pairing) = self.pairing() {
            info!("found existing topic {pairing}");
            self.resubscribe().await?;
            info!("Checking if peer is alive");
            if !self.alive().await {
                info!("clearing pairing topics and sessions");
                self.relay.unsubscribe(pairing.topic.clone()).await?;
                self.ciphers.set_pairing(None)?;
                return Ok(());
            }
        }
        Ok(())
    }

    async fn register_pk(&self, pk: String) -> Result<SessionTopic> {
        let (session_topic, _) = self.ciphers.create_common_topic(pk)?;
        // TODO: Do I need the subscriptionId?
        self.subscribe(session_topic.clone()).await?;
        Ok(session_topic)
    }

    pub(crate) async fn register_wallet_pk(
        &self,
        controller: SessionProposeResponse,
    ) -> Result<Topic> {
        self.register_pk(controller.responder_public_key).await
    }

    pub(crate) async fn register_dapp_pk(&self, proposer: Proposer) -> Result<Topic> {
        self.register_pk(proposer.public_key).await
    }
}
