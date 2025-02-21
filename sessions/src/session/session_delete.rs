use {
    crate::{rpc::SessionDeleteRequest, ClientSession, SessionDeleteHandler},
    tokio::sync::mpsc,
    tracing::info,
    xtra::prelude::*,
};

#[allow(dead_code)]
pub async fn handle_delete<T: SessionDeleteHandler>(
    handler: T,
    mut rx: mpsc::Receiver<SessionDeleteRequest>,
) {
    while let Some(message) = rx.recv().await {
        handler.handle(message).await;
    }
}

impl Handler<SessionDeleteRequest> for ClientSession {
    type Return = ();

    async fn handle(
        &mut self,
        message: SessionDeleteRequest,
        _ctx: &mut Context<Self>,
    ) -> Self::Return {
        info!("session delete requested {message:#?}");
    }
}
