use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::sync::{Arc, Once};
use std::time::Duration;

use anyhow::format_err;
use assert_matches::assert_matches;
use async_trait::async_trait;
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use monedero_mesh::crypto::CipherError;
use monedero_mesh::rpc::{
    Metadata, ResponseParamsError, ResponseParamsSuccess, RpcResponsePayload,
    SessionProposeRequest, SessionProposeResponse, SessionRequestRequest,
};
use monedero_mesh::{
    Actors, ClientSession, Dapp, NoopSessionHandler, ProjectId, ProposeFuture,
    RegisteredComponents, SdkErrors, Topic, Wallet, WalletConnectBuilder, WalletRequestResponse,
    WalletSettlementHandler,
};
use monedero_namespaces::{
    Account, Accounts, AlloyChain, ChainId, ChainType, Chains, EipMethod, Events, Method, Methods,
    Namespace, NamespaceName, Namespaces, SolanaMethod,
};
use monedero_relay::{auth_token, ConnectionCategory, ConnectionOptions, ConnectionPair};
use monedero_solana::{
    Error, ReownSigner, Result, SolanaSession, SolanaSignatureResponse, StakeClient,
    TokenMintClient, TokenSymbolDev, TokenTransferClient, WalletConnectTransaction,
};
use serde::Deserialize;
use solana_program::native_token::LAMPORTS_PER_SOL;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_rpc_client::rpc_client::{RpcClientConfig, SerializableTransaction};
use solana_sdk::message::Message;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::transaction::{Transaction, VersionedTransaction};
use tokio::time::timeout;
use tracing::{error, info};
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::EnvFilter;

#[allow(dead_code)]
static INIT: Once = Once::new();

pub(crate) async fn yield_ms(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

#[derive(Clone)]
struct MockWallet {
    rpc_client: Arc<RpcClient>,
}

const SUPPORTED_ACCOUNT: &str = "215r9xfTFVYcE9g3fAUGowauM84egyUvFCbSo3LKNaep";

#[async_trait]
impl WalletSettlementHandler for MockWallet {
    async fn settlement(
        &self,
        proposal: SessionProposeRequest,
    ) -> monedero_mesh::Result<Namespaces> {
        let mut settled: Namespaces = Namespaces(BTreeMap::new());
        for (name, namespace) in proposal.required_namespaces.iter() {
            let accounts: BTreeSet<Account> = namespace
                .chains
                .iter()
                .map(|c| Account {
                    address: String::from(SUPPORTED_ACCOUNT),
                    chain: c.clone(),
                })
                .collect();

            let methods = match name {
                NamespaceName::EIP155 => EipMethod::defaults(),
                NamespaceName::Solana => SolanaMethod::defaults(),
                NamespaceName::Other(_) => BTreeSet::from([Method::Other("unknown".to_owned())]),
            };
            settled.insert(
                name.clone(),
                Namespace {
                    accounts: Accounts(accounts),
                    chains: Chains(namespace.chains.iter().cloned().collect()),
                    methods: Methods(methods),
                    events: Events::default(),
                },
            );
        }
        Ok(settled)
    }
}

impl monedero_mesh::SessionEventHandler for MockWallet {}

const KEYPAIR: [u8; 64] = [
    186, 128, 232, 61, 254, 246, 197, 13, 125, 103, 212, 83, 16, 121, 144, 20, 93, 161, 35, 128,
    89, 135, 157, 200, 81, 159, 83, 204, 204, 130, 28, 42, 14, 225, 43, 2, 44, 16, 255, 214, 161,
    18, 184, 164, 253, 126, 16, 187, 134, 176, 75, 35, 179, 194, 181, 150, 67, 236, 131, 49, 45,
    155, 45, 253,
];

impl MockWallet {
    pub fn pk(&self) -> Pubkey {
        let kp = Keypair::from_bytes(&KEYPAIR).unwrap();
        kp.pubkey()
    }

    pub async fn sign(&self, value: serde_json::Value) -> Result<SolanaSignatureResponse> {
        let kp = Keypair::from_bytes(&KEYPAIR).map_err(|_| Error::KeyPairFailure)?;
        info!("PK of signer: {}", kp.pubkey());
        let req = serde_json::from_value::<WalletConnectTransaction>(value)?;
        let decoded = BASE64_STANDARD.decode(req.transaction)?;
        let mut tx = bincode::deserialize::<Transaction>(decoded.as_ref())?;
        let positions = tx.get_signing_keypair_positions(&[kp.pubkey()])?;
        if positions.is_empty() {
            return Err(Error::NothingToSign);
        }
        tx.try_partial_sign(&[&kp], tx.get_recent_blockhash().clone())?;
        //tx.try_sign(&[&kp], tx.get_recent_blockhash().clone())?;
        let sig = tx.get_signature();
        let signature = solana_sdk::bs58::encode(sig).into_string();
        info!("returning sig: {signature}");
        Ok(SolanaSignatureResponse { signature })
    }
}

#[async_trait]
impl monedero_mesh::SessionHandler for MockWallet {
    async fn request(&self, request: SessionRequestRequest) -> WalletRequestResponse {
        match request.request.method {
            Method::Solana(SolanaMethod::SignTransaction) => {
                match self.sign(request.request.params).await {
                    Err(e) => {
                        tracing::warn!("failed sig: {e}");
                        WalletRequestResponse::Error(SdkErrors::UserRejected)
                    }
                    Ok(sig) => WalletRequestResponse::Success(serde_json::to_value(&sig).unwrap()),
                }
            }
            _ => WalletRequestResponse::Error(SdkErrors::InvalidMethod),
        }
    }
}

pub(crate) async fn init_test_components() -> anyhow::Result<(Dapp, Wallet, MockWallet)> {
    init_tracing();
    let shared_id = Topic::generate();
    let p = ProjectId::from("9d5b20b16777cc49100cf9df3649bd24");
    let auth = auth_token("https://github.com/dougEfresh");
    let dapp_id = ConnectionPair(shared_id.clone(), ConnectionCategory::Dapp);
    let wallet_id = ConnectionPair(shared_id.clone(), ConnectionCategory::Wallet);
    let dapp_opts = ConnectionOptions::new(p.clone(), auth.clone(), dapp_id);
    let wallet_opts = ConnectionOptions::new(p.clone(), auth.clone(), wallet_id);
    let dapp_manager = WalletConnectBuilder::new(p.clone(), auth.clone())
        .connect_opts(dapp_opts)
        .build()
        .await?;
    let wallet_manager = WalletConnectBuilder::new(p, auth)
        .connect_opts(wallet_opts)
        .build()
        .await?;
    let md = Metadata {
        name: "mock-dapp".to_string(),
        ..Default::default()
    };
    let dapp = Dapp::new(dapp_manager, md).await?;
    dotenvy::dotenv()?;
    //let url = std::env::var(ChainId::Solana(ChainType::Test).to_string())
    //.ok()
    //.unwrap_or(String::from("https://api.devnet.solana.com"));
    let url = std::env::var(ChainId::Solana(ChainType::Dev).to_string())
        .ok()
        .unwrap_or(String::from("https://soldev.dougchimento.com"));
    info!("using url {url}");
    let rpc_client = Arc::new(RpcClient::new(url));
    let mock_wallet = MockWallet { rpc_client };
    let wallet = Wallet::new(wallet_manager, mock_wallet.clone()).await?;
    Ok((dapp, wallet, mock_wallet))
}

pub(crate) fn init_tracing() {
    INIT.call_once(|| {
        tracing_subscriber::fmt()
            .with_target(true)
            .with_level(true)
            .with_span_events(FmtSpan::CLOSE)
            .with_env_filter(EnvFilter::from_default_env())
            .init();
    });
}

async fn await_wallet_pair(rx: ProposeFuture) {
    match timeout(Duration::from_secs(5), rx).await {
        Ok(s) => match s {
            Ok(_) => {
                info!("wallet got client session")
            }
            Err(e) => error!("wallet got error session: {e}"),
        },
        Err(e) => error!("timeout for wallet to recv client session from wallet: {e}"),
    }
}

async fn pair_dapp_wallet() -> anyhow::Result<(ClientSession, MockWallet)> {
    let (dapp, wallet, mock_wallet) = init_test_components().await?;
    let (pairing, rx, _) = dapp
        .propose(NoopSessionHandler, &[ChainId::Solana(ChainType::Dev)])
        .await?;
    info!("got pairing topic {pairing}");
    let (_, wallet_rx) = wallet
        .pair(pairing.to_string(), mock_wallet.clone())
        .await?;
    tokio::spawn(async move { await_wallet_pair(wallet_rx).await });
    let session = timeout(Duration::from_secs(5), rx).await??;
    // let wallet get their ClientSession
    yield_ms(1000).await;
    Ok((session, mock_wallet))
}
#[tokio::test(flavor = "multi_thread", worker_threads = 10)]

async fn test_solana_session() -> anyhow::Result<()> {
    let (session, mock_wallet) = pair_dapp_wallet().await?;
    info!("settlement complete");
    let sol_session = SolanaSession::try_from(&session)?;
    let signer = ReownSigner::new(sol_session.clone());
    let from = Pubkey::from_str(SUPPORTED_ACCOUNT)?;
    let to = Pubkey::from_str("E4SfgGV2v9GLYsEkCQhrrnFbBcYmAiUZZbJ7swKGzZHJ")?;
    let amount = 100;
    let balance = mock_wallet.rpc_client.get_balance(&from).await?;
    info!("balance in lamports {balance}");
    let instruction = solana_sdk::system_instruction::transfer(&sol_session.pubkey(), &to, amount);
    let message = Message::new(&[instruction], Some(&from));
    let block = mock_wallet.rpc_client.get_latest_blockhash().await?;
    info!("dapp using block {block}");
    let tx = Transaction::new(&[&signer], message, block);
    //let kp = Keypair::from_bytes(&KEYPAIR).map_err(|_| Error::KeyPairFailure)?;
    //let tx = solana_sdk::system_transaction::transfer(&kp, &to, 100, block);
    info!("sending transaction...");
    let sig = mock_wallet
        .rpc_client
        .send_and_confirm_transaction(&tx)
        .await?;
    info!("got sig {sig}");

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 10)]
async fn test_solana_tokens() -> anyhow::Result<()> {
    let (session, mock_wallet) = pair_dapp_wallet().await?;
    let sol_session = SolanaSession::try_from(&session)?;
    let signer = ReownSigner::new(sol_session);
    let token_client = TokenTransferClient::init(
        signer,
        mock_wallet.rpc_client.clone(),
        TokenSymbolDev::USDC,
        spl_token::id(),
    )
    .await?;
    let balance = token_client.balance().await?;
    let to = Pubkey::from_str("E4SfgGV2v9GLYsEkCQhrrnFbBcYmAiUZZbJ7swKGzZHJ")?;
    info!(
        "balance {} on token account {}  (wallet:{})",
        balance,
        token_client.account(),
        mock_wallet.pk()
    );
    info!("sending to {to}");
    let sig = token_client.transfer(&to, 1).await?;
    info!("got signature {sig}");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 10)]
async fn test_solana_mint() -> anyhow::Result<()> {
    let (session, mock_wallet) = pair_dapp_wallet().await?;
    let sol_session = SolanaSession::try_from(&session)?;
    let signer = ReownSigner::new(sol_session);
    let to = Pubkey::from_str("E4SfgGV2v9GLYsEkCQhrrnFbBcYmAiUZZbJ7swKGzZHJ")?;
    let mint = TokenMintClient::new(mock_wallet.rpc_client.clone(), signer.clone());
    let (token_address, sig) = mint.create_mint(6, None).await?;
    info!("created mint {token_address} signature: {sig}");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 10)]
async fn test_solana_wrap_sol() -> anyhow::Result<()> {
    let (session, mock_wallet) = pair_dapp_wallet().await?;
    let sol_session = SolanaSession::try_from(&session)?;
    let signer = ReownSigner::new(sol_session);
    let token_client = TokenTransferClient::init_wrap_native(
        signer,
        mock_wallet.rpc_client.clone(),
        spl_token::id(),
    );
    info!("wrapped account {}", token_client.account());
    let wrapped = token_client.wrap(5000, false).await?;
    let balance = token_client.balance().await?;
    info!("immutable wrapped {wrapped} balance: {balance}");

    let wrapped = token_client.wrap(5000, true).await?;
    let balance = token_client.balance().await?;
    info!("mut wrapped {wrapped} balance: {balance}");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 10)]
async fn test_solana_mint_new_tokens() -> anyhow::Result<()> {
    // this came from  test_solana_mint above
    let token_address = Pubkey::from_str("8m3uKEn4fMPNVr7nv6RmQYktT4zRqEZzhuZDpG8hQZT4")?;
    let (session, mock_wallet) = pair_dapp_wallet().await?;
    let sol_session = SolanaSession::try_from(&session)?;
    let signer = Arc::new(ReownSigner::new(sol_session));
    let me = Pubkey::from_str(SUPPORTED_ACCOUNT)?;
    let to = Pubkey::from_str("E4SfgGV2v9GLYsEkCQhrrnFbBcYmAiUZZbJ7swKGzZHJ")?;
    let token_client = TokenTransferClient::init(
        signer,
        mock_wallet.rpc_client.clone(),
        token_address,
        spl_token::id(),
    )
    .await?;
    let sig = token_client.mint_to(&me, 100_000_000).await?;
    info!("signature for minting to me {sig}");
    let sig = token_client.transfer(&to, 1000000).await?;
    info!("signature for sending to {to} {sig}");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 10)]
async fn test_solana_stake_accounts() -> anyhow::Result<()> {
    let (session, mock_wallet) = pair_dapp_wallet().await?;
    info!("settlement complete");
    let sol_session = SolanaSession::try_from(&session)?;
    let signer = ReownSigner::new(sol_session.clone());
    let staker = StakeClient::new(sol_session, signer, mock_wallet.rpc_client.clone());
    let validators = staker.validators().await?;
    info!("there are {} validators", validators.len());
    let accounts = staker.accounts().await?;
    info!("you have {} staked accounts", accounts.len());
    for a in &accounts {
        info!("{a}")
    }
    let seed: &str = "seed";
    let (account, sig) = staker.create_account(seed, LAMPORTS_PER_SOL).await?;
    info!("create new account {account} with seed {seed}  sig: {sig}");
    Ok(())
}
