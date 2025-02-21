use {
    crate::{
        actors::RequestHandlerActor,
        rpc::{IntoUnknownError, RpcResponse, RpcResponsePayload},
        spawn_task,
        PairingManager,
        Result,
        Topic,
    },
    monedero_domain::MessageId,
    tracing::warn,
};

impl RequestHandlerActor {
    pub(super) fn send_response(&self, resp: RpcResponse) {
        let me = self.clone();
        let id = resp.id;
        let topic = resp.topic.clone();
        spawn_task(async move {
            if let Err(err) = me.responder.send(resp).await {
                warn!(
                    "Failed to send response for id {} on topic {} {}",
                    id, topic, err
                );
            }
        });
    }

    async fn internal_handle_pair_request<M>(
        &self,
        id: MessageId,
        topic: Topic,
        request: M,
    ) -> Result<()>
    where
        M: Send + 'static,
        PairingManager: xtra::Handler<M>,
        <PairingManager as xtra::Handler<M>>::Return: Into<RpcResponsePayload>,
    {
        let mgr = self
            .pair_managers
            .as_ref()
            .ok_or(crate::Error::NoPairManager(topic.clone()))?;
        let response: RpcResponse = mgr.send(request).await.map(|r| RpcResponse {
            id,
            topic: topic.clone(),
            payload: r.into(),
        })?;
        self.send_response(response);
        Ok(())
    }

    pub(super) async fn handle_pair_mgr_request<M>(&self, id: MessageId, topic: Topic, request: M)
    where
        M: IntoUnknownError + Send + 'static,
        PairingManager: xtra::Handler<M>,
        <PairingManager as xtra::Handler<M>>::Return: Into<RpcResponsePayload>,
    {
        let u: RpcResponse = RpcResponse::unknown(id, topic.clone(), request.unknown());
        if let Err(e) = self.internal_handle_pair_request(id, topic, request).await {
            warn!("failed to get response from pair manager: '{e}'");
            self.send_response(u);
        }
    }
}
