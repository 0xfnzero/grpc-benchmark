use grpc_benchmark::{
    comparison::{
        entries::{decode_entries, EntryMessage, SubscribeEntriesRequest, DEFAULT_ENTRIES_METHOD},
        sources::{SourceConfig, Transport},
        stats::{Comparator, ComparisonConfig},
        Mode, SampleKey,
    },
    shred_client::{RawShredConfig, ShredDecodeMode},
};
use solana_entry::entry::Entry;
use solana_sdk::{
    hash::Hash,
    instruction::Instruction,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use std::{
    convert::Infallible,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    net::{TcpListener, UdpSocket},
    sync::{mpsc, oneshot},
};
use tokio_stream::{wrappers::TcpListenerStream, Stream, StreamExt};
use tonic::{
    codegen::{http, BoxFuture, Service},
    server::{NamedService, ServerStreamingService},
    Request, Response, Status,
};

fn signed_entries() -> (Vec<u8>, [u8; 64]) {
    let signer = Keypair::new();
    let tx = Transaction::new_signed_with_payer(
        &[Instruction {
            program_id: solana_sdk::pubkey::Pubkey::new_unique(),
            accounts: vec![],
            data: vec![1, 2, 3],
        }],
        Some(&signer.pubkey()),
        &[&signer],
        Hash::new_unique(),
    );
    tx.verify().expect("fixture must be genuinely signed");
    let signature = *tx.signatures[0].as_array();
    let entry = Entry {
        num_hashes: 1,
        hash: Hash::new_unique(),
        transactions: vec![tx.into()],
    };
    (bincode::serialize(&vec![entry]).unwrap(), signature)
}

fn signed_v0_entries() -> (Vec<u8>, [u8; 64]) {
    use solana_sdk::{
        address_lookup_table::AddressLookupTableAccount,
        instruction::AccountMeta,
        message::{v0, VersionedMessage},
        transaction::VersionedTransaction,
    };
    let signer = Keypair::new();
    let recipient = solana_sdk::pubkey::Pubkey::new_unique();
    let instruction = Instruction {
        program_id: solana_sdk::pubkey::Pubkey::new_unique(),
        accounts: vec![AccountMeta::new(recipient, false)],
        data: vec![3, 4],
    };
    let table = AddressLookupTableAccount {
        key: solana_sdk::pubkey::Pubkey::new_unique(),
        addresses: vec![recipient],
    };
    let message = v0::Message::try_compile(
        &signer.pubkey(),
        &[instruction],
        &[table],
        Hash::new_unique(),
    )
    .unwrap();
    assert_eq!(message.address_table_lookups.len(), 1);
    let transaction =
        VersionedTransaction::try_new(VersionedMessage::V0(message), &[&signer]).unwrap();
    transaction.verify_and_hash_message().unwrap();
    let signature = *transaction.signatures[0].as_array();
    (
        bincode::serialize(&vec![Entry {
            num_hashes: 1,
            hash: Hash::new_unique(),
            transactions: vec![transaction],
        }])
        .unwrap(),
        signature,
    )
}

// Legacy data shred wire format; payloads are real signed Vec<Entry>, split at arbitrary boundaries.
fn packets(bytes: &[u8], slot: u64) -> Vec<Vec<u8>> {
    let chunks: Vec<_> = bytes.chunks(100).collect();
    chunks
        .iter()
        .enumerate()
        .map(|(i, data)| {
            let mut packet = vec![0; 88 + data.len()];
            packet[64] = 0xa5;
            packet[65..73].copy_from_slice(&slot.to_le_bytes());
            packet[73..77].copy_from_slice(&(i as u32).to_le_bytes());
            packet[77..79].copy_from_slice(&1u16.to_le_bytes());
            if i == chunks.len() - 1 {
                packet[85] = 0xc0;
            }
            let len = packet.len() as u16;
            packet[86..88].copy_from_slice(&len.to_le_bytes());
            packet[88..].copy_from_slice(data);
            packet
        })
        .collect()
}

fn udp_config(mode: ShredDecodeMode, prefix: usize) -> SourceConfig {
    SourceConfig {
        transport: Transport::Udp,
        udp: RawShredConfig {
            bind_addr: "127.0.0.1:0".into(),
            decode_mode: mode,
            udp_payload_prefix_skip: prefix,
            ..Default::default()
        },
        url: None,
        method: DEFAULT_ENTRIES_METHOD.into(),
        token: None,
        token_header: "x-token".into(),
    }
}

#[test]
fn bounded_entry_decoder_rejects_malformed_truncated_trailing_and_excessive_counts() {
    let (bytes, sig) = signed_entries();
    let decoded = decode_entries(&bytes).unwrap();
    assert_eq!(*decoded[0].transactions[0].signatures[0].as_array(), sig);
    assert!(decode_entries(&bytes[..bytes.len() - 1]).is_err());
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(decode_entries(&trailing).is_err());
    assert!(decode_entries(&50001u64.to_le_bytes()).is_err());
    assert!(decode_entries(&[]).is_err());
    let message = EntryMessage {
        slot: 123,
        entries: vec![1, 2],
    };
    // Check the canonical field tags independently of the transport client.
    assert_eq!(
        prost::Message::encode_to_vec(&message),
        vec![8, 123, 18, 2, 1, 2]
    );
}

#[tokio::test]
async fn udp_modes_decode_signed_transactions_with_prefix_reordering_and_duplicates() {
    for decode_mode in [ShredDecodeMode::Ledger, ShredDecodeMode::Compat] {
        for mode in [Mode::Slot, Mode::Shred, Mode::Transaction] {
            let source = udp_config(decode_mode, 2).prepare(mode).await.unwrap();
            let addr = source.local_addr().unwrap().unwrap();
            let (tx, mut rx) = mpsc::channel(256);
            let stop = Instant::now() + Duration::from_millis(180);
            let handle = tokio::spawn(source.run(0, mode, stop, tx));
            let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
            let (bytes, sig) = signed_entries();
            let mut shreds = packets(&bytes, 123);
            // One out-of-order packet followed by the complete stream and a retransmission.
            shreds.insert(0, shreds[1].clone());
            shreds.push(shreds[0].clone());
            sender.send_to(&[0; 4], addr).await.unwrap();
            for packet in shreds {
                let mut prefixed = vec![0x11, 0x22];
                prefixed.extend(packet);
                sender.send_to(&prefixed, addr).await.unwrap();
            }
            let stats = handle.await.unwrap().unwrap();
            assert_eq!(stats.invalid, 1);
            let mut observations = vec![];
            while let Some(item) = rx.recv().await {
                observations.push(item);
            }
            assert!(!observations.is_empty(), "{decode_mode:?} {mode:?}");
            match mode {
                Mode::Transaction => assert!(observations.iter().any(|e| e.key
                    == SampleKey::Transaction {
                        slot: 123,
                        signature: sig
                    })),
                Mode::Shred => assert!(observations.iter().any(|e| e.key
                    == SampleKey::Shred {
                        slot: 123,
                        index: 0,
                        coding: false
                    })),
                Mode::Slot => assert!(observations.iter().all(|e| e.key == SampleKey::Slot(123))),
            }
        }
    }
}

#[derive(Clone)]
struct MockEntries {
    messages: Arc<Vec<EntryMessage>>,
    finite: bool,
}
impl NamedService for MockEntries {
    const NAME: &'static str = "shredstream.ShredstreamProxy";
}
impl Service<http::Request<tonic::body::BoxBody>> for MockEntries {
    type Response = http::Response<tonic::body::BoxBody>;
    type Error = Infallible;
    type Future = BoxFuture<Self::Response, Self::Error>;
    fn poll_ready(
        &mut self,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }
    fn call(&mut self, req: http::Request<tonic::body::BoxBody>) -> Self::Future {
        struct Subscribe(Arc<Vec<EntryMessage>>, bool);
        impl ServerStreamingService<SubscribeEntriesRequest> for Subscribe {
            type Response = EntryMessage;
            type ResponseStream = Pin<Box<dyn Stream<Item = Result<EntryMessage, Status>> + Send>>;
            type Future = BoxFuture<Response<Self::ResponseStream>, Status>;
            fn call(&mut self, req: Request<SubscribeEntriesRequest>) -> Self::Future {
                let messages = self.0.as_ref().clone();
                let finite = self.1;
                Box::pin(async move {
                    if req
                        .metadata()
                        .get("x-api-key")
                        .and_then(|v| v.to_str().ok())
                        != Some("test-secret")
                    {
                        return Err(Status::unauthenticated("missing fixture auth"));
                    }
                    let stream = tokio_stream::iter(messages.into_iter().map(Ok));
                    let stream: Self::ResponseStream = if finite {
                        Box::pin(stream)
                    } else {
                        Box::pin(stream.chain(tokio_stream::pending()))
                    };
                    Ok(Response::new(stream))
                })
            }
        }
        let messages = self.messages.clone();
        let finite = self.finite;
        let valid_path = [
            DEFAULT_ENTRIES_METHOD,
            "/shredstream.ShredstreamProxy/CustomEntries",
        ]
        .contains(&req.uri().path());
        Box::pin(async move {
            if !valid_path {
                return Ok(http::Response::builder()
                    .status(200)
                    .header("grpc-status", "12")
                    .header("content-type", "application/grpc")
                    .body(tonic::body::empty_body())
                    .unwrap());
            }
            let codec = tonic::codec::ProstCodec::default();
            let mut grpc = tonic::server::Grpc::new(codec);
            Ok(grpc
                .server_streaming(Subscribe(messages, finite), req)
                .await)
        })
    }
}

async fn mock_server(
    messages: Vec<EntryMessage>,
) -> (String, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    mock_server_with_eof(messages, false).await
}
async fn mock_server_with_eof(
    messages: Vec<EntryMessage>,
    finite: bool,
) -> (String, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown, rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(MockEntries {
                messages: Arc::new(messages),
                finite,
            })
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async {
                let _ = rx.await;
            })
            .await
            .unwrap();
    });
    (format!("http://{addr}"), shutdown, task)
}
fn entries_config(url: String, custom: bool) -> SourceConfig {
    SourceConfig {
        transport: Transport::EntriesGrpc,
        udp: Default::default(),
        url: Some(url),
        method: if custom {
            "/shredstream.ShredstreamProxy/CustomEntries".into()
        } else {
            DEFAULT_ENTRIES_METHOD.into()
        },
        token: Some("test-secret".into()),
        token_header: "x-api-key".into(),
    }
}

#[tokio::test]
async fn entry_grpc_and_udp_match_the_same_signed_transaction_without_proxy() {
    let (bytes, sig) = signed_v0_entries();
    let (url, shutdown, server) = mock_server(vec![EntryMessage {
        slot: 123,
        entries: bytes.clone(),
    }])
    .await;
    let grpc = entries_config(url.clone(), true)
        .prepare(Mode::Transaction)
        .await
        .unwrap();
    let udp = udp_config(ShredDecodeMode::Ledger, 0)
        .prepare(Mode::Transaction)
        .await
        .unwrap();
    let addr = udp.local_addr().unwrap().unwrap();
    let (tx, mut rx) = mpsc::channel(256);
    let start = Instant::now();
    let stop = start + Duration::from_millis(220);
    let a = tokio::spawn(grpc.run(0, Mode::Transaction, stop, tx.clone()));
    let b = tokio::spawn(udp.run(1, Mode::Transaction, stop, tx));
    let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    for packet in packets(&bytes, 123) {
        sender.send_to(&packet, addr).await.unwrap();
    }
    let mut cmp = Comparator::new(
        ComparisonConfig {
            mode: Mode::Transaction,
            duration: Duration::from_secs(1),
            settle: Duration::from_secs(1),
            max_keys: 100,
            max_samples: 100,
            retention_slots: 128,
        },
        ["Entry".into(), "UDP".into()],
        start,
    );
    let mut found = [false; 2];
    while let Some(event) = rx.recv().await {
        assert_eq!(
            event.key,
            SampleKey::Transaction {
                slot: 123,
                signature: sig
            }
        );
        found[event.source] = true;
        cmp.record(event).unwrap();
    }
    assert!(found.into_iter().all(|v| v));
    assert_eq!(cmp.summary().unwrap().matched, 1);
    a.await.unwrap().unwrap();
    b.await.unwrap().unwrap();
    let mut bad = entries_config(url, false);
    bad.token = None;
    assert!(bad.prepare(Mode::Transaction).await.is_err());
    let _ = shutdown.send(());
    server.await.unwrap();
}

#[tokio::test]
async fn entry_slot_mode_validates_payload_and_shred_mode_is_rejected() {
    let (bytes, _) = signed_entries();
    let (url, shutdown, server) = mock_server(vec![
        EntryMessage {
            slot: 123,
            entries: vec![1],
        },
        EntryMessage {
            slot: 124,
            entries: bytes,
        },
    ])
    .await;
    assert!(entries_config(url.clone(), false)
        .prepare(Mode::Shred)
        .await
        .is_err());
    let source = entries_config(url, false)
        .prepare(Mode::Slot)
        .await
        .unwrap();
    let (tx, mut rx) = mpsc::channel(10);
    let stats = source
        .run(
            0,
            Mode::Slot,
            Instant::now() + Duration::from_millis(100),
            tx,
        )
        .await
        .unwrap();
    assert_eq!(stats.invalid, 1);
    let item = rx.recv().await.unwrap();
    assert_eq!(item.key, SampleKey::Slot(124));
    assert!(rx.recv().await.is_none());
    let _ = shutdown.send(());
    server.await.unwrap();
}

fn spawn_cli(
    mut command: std::process::Command,
) -> (
    tokio::task::JoinHandle<std::process::Output>,
    oneshot::Receiver<()>,
) {
    use std::io::{BufRead, BufReader};
    let (ready, rx) = oneshot::channel();
    let output = tokio::task::spawn_blocking(move || {
        let mut child = command
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut captured = Vec::new();
        let mut ready = Some(ready);
        for line in BufReader::new(stdout).lines() {
            let line = line.unwrap();
            captured.extend_from_slice(line.as_bytes());
            captured.push(b'\n');
            if line.starts_with("Both sources ready") {
                if let Some(ready) = ready.take() {
                    let _ = ready.send(());
                }
            }
        }
        let mut output = child.wait_with_output().unwrap();
        output.stdout = captured;
        output
    });
    (output, rx)
}

fn report_path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "shred-comparison-{}-{label}.json",
        std::process::id()
    ))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_entry_vs_entry_and_mixed_udp_entry_reports_are_usable() {
    let (bytes, _) = signed_entries();
    for mixed in [false, true] {
        let (url, shutdown, server) = mock_server(vec![EntryMessage {
            slot: 123,
            entries: bytes.clone(),
        }])
        .await;
        let path = report_path(if mixed { "mixed" } else { "entries" });
        let udp_reservation = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let udp_addr = udp_reservation.local_addr().unwrap();
        drop(udp_reservation);
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_shred-comparison"));
        command.args([
            "--mode",
            "transaction",
            "--source-1",
            if mixed { "udp" } else { "entries-grpc" },
            "--source-2",
            "entries-grpc",
            "--shred-url-1",
            &url,
            "--shred-url-2",
            &url,
            "--shred-bind-1",
            &udp_addr.to_string(),
            "--duration-sec",
            "1",
            "--warmup-ms",
            "0",
            "--settle-window-ms",
            "100",
            "--json-output",
            path.to_str().unwrap(),
        ]);
        command
            .env("SHRED_TOKEN_1", "test-secret")
            .env("SHRED_TOKEN_2", "test-secret")
            .env("SHRED_TOKEN_HEADER_1", "x-api-key")
            .env("SHRED_TOKEN_HEADER_2", "x-api-key");
        let (output, ready) = spawn_cli(command);
        tokio::time::timeout(Duration::from_secs(10), ready)
            .await
            .unwrap()
            .unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        if mixed {
            // Retry during startup: UDP is connectionless and the CLI binds asynchronously.
            for _ in 0..30 {
                for packet in packets(&bytes, 123) {
                    sender.send_to(&packet, udp_addr).await.unwrap();
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
        let output = output.await.unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let raw = std::fs::read_to_string(&path).unwrap();
        let report: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(report["valid"], true);
        assert_eq!(report["comparison"]["matched"], 1);
        assert!(!raw.contains("test-secret"));
        assert!(!raw.contains(&url));
        std::fs::remove_file(path).unwrap();
        let _ = shutdown.send(());
        server.await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_no_matches_writes_invalid_report_and_bad_configuration_fails_early() {
    let listener_a = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let listener_b = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let a = listener_a.local_addr().unwrap();
    let b = listener_b.local_addr().unwrap();
    drop(listener_a);
    drop(listener_b);
    let path = report_path("empty");
    let path_clone = path.clone();
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new(env!("CARGO_BIN_EXE_shred-comparison"))
            .args([
                "--mode",
                "shred",
                "--shred-bind-1",
                &a.to_string(),
                "--shred-bind-2",
                &b.to_string(),
                "--duration-sec",
                "1",
                "--warmup-ms",
                "0",
                "--settle-window-ms",
                "10",
                "--json-output",
                path_clone.to_str().unwrap(),
            ])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(report["valid"], false);
    assert_eq!(report["comparison"]["matched"], 0);
    assert!(report["comparison"]["b_minus_a_us"].is_null());
    std::fs::remove_file(path).unwrap();
    for args in [
        vec!["--mode", "shred", "--source-1", "entries-grpc"],
        vec!["--shred-name-1", "A", "--shred-name-2", " A "],
        vec!["--max-keys", "0"],
        vec!["--duration-sec", "0"],
        vec![
            "--shred-bind-1",
            "127.0.0.1:20000",
            "--shred-bind-2",
            "0.0.0.0:20000",
        ],
    ] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_shred-comparison"))
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
    }
}

#[tokio::test]
async fn entry_stream_eof_and_unknown_rpc_method_are_errors() {
    let (bytes, _) = signed_entries();
    let (url, shutdown, server) = mock_server_with_eof(
        vec![EntryMessage {
            slot: 123,
            entries: bytes,
        }],
        true,
    )
    .await;
    let mut config = entries_config(url.clone(), false);
    config.method = "/shredstream.ShredstreamProxy/NotImplemented".into();
    assert!(config.prepare(Mode::Transaction).await.is_err());
    let source = entries_config(url, false)
        .prepare(Mode::Transaction)
        .await
        .unwrap();
    let (tx, _rx) = mpsc::channel(10);
    let result = source
        .run(
            0,
            Mode::Transaction,
            Instant::now() + Duration::from_secs(1),
            tx,
        )
        .await;
    assert!(
        result.is_err(),
        "an early EOF must not become a successful benchmark"
    );
    let _ = shutdown.send(());
    server.await.unwrap();
}

#[tokio::test]
async fn entry_streams_with_different_entry_batches_match_actual_signed_transactions() {
    let (legacy, sig_a) = signed_entries();
    let (v0, sig_b) = signed_v0_entries();
    let mut combined = decode_entries(&legacy).unwrap();
    combined.extend(decode_entries(&v0).unwrap());
    let (url_a, shutdown_a, server_a) = mock_server(vec![EntryMessage {
        slot: 123,
        entries: bincode::serialize(&combined).unwrap(),
    }])
    .await;
    let (url_b, shutdown_b, server_b) = mock_server(vec![
        EntryMessage {
            slot: 123,
            entries: v0.clone(),
        },
        EntryMessage {
            slot: 123,
            entries: legacy,
        },
        EntryMessage {
            slot: 123,
            entries: v0,
        },
    ])
    .await;
    let a = entries_config(url_a, false)
        .prepare(Mode::Transaction)
        .await
        .unwrap();
    let b = entries_config(url_b, false)
        .prepare(Mode::Transaction)
        .await
        .unwrap();
    let (tx, mut rx) = mpsc::channel(32);
    let start = Instant::now();
    let stop = start + Duration::from_millis(120);
    let handle_a = tokio::spawn(a.run(0, Mode::Transaction, stop, tx.clone()));
    let handle_b = tokio::spawn(b.run(1, Mode::Transaction, stop, tx));
    let mut comparator = Comparator::new(
        ComparisonConfig {
            mode: Mode::Transaction,
            duration: Duration::from_secs(1),
            settle: Duration::from_secs(1),
            max_keys: 100,
            max_samples: 100,
            retention_slots: 128,
        },
        ["A".into(), "B".into()],
        start,
    );
    while let Some(observation) = rx.recv().await {
        assert!(
            matches!(&observation.key,SampleKey::Transaction{signature,..} if *signature==sig_a||*signature==sig_b)
        );
        comparator.record(observation).unwrap();
    }
    let summary = comparator.summary().unwrap();
    assert_eq!(summary.matched, 2);
    assert_eq!(summary.sources[1].duplicates, 1);
    handle_a.await.unwrap().unwrap();
    handle_b.await.unwrap().unwrap();
    let _ = shutdown_a.send(());
    let _ = shutdown_b.send(());
    server_a.await.unwrap();
    server_b.await.unwrap();
}

#[tokio::test]
async fn udp_merkle_fec_recovers_missing_data_and_exports_signed_transaction() {
    let (bytes, signature) = signed_entries();
    let split = bytes.len() / 2;
    let mut data: Vec<Vec<u8>> = vec![vec![0; 1203], vec![0; 1203]];
    for (i, payload) in [&bytes[..split], &bytes[split..]].into_iter().enumerate() {
        let packet = &mut data[i];
        packet[64] = 0x80;
        packet[65..73].copy_from_slice(&123u64.to_le_bytes());
        packet[73..77].copy_from_slice(&(i as u32).to_le_bytes());
        packet[77..79].copy_from_slice(&1u16.to_le_bytes());
        if i == 1 {
            packet[85] = 0xc0;
        }
        packet[86..88].copy_from_slice(&((88 + payload.len()) as u16).to_le_bytes());
        packet[88..88 + payload.len()].copy_from_slice(payload);
    }
    let mut shards = vec![
        data[0][64..].to_vec(),
        data[1][64..].to_vec(),
        vec![0; 1139],
    ];
    reed_solomon_erasure::galois_8::ReedSolomon::new(2, 1)
        .unwrap()
        .encode(&mut shards)
        .unwrap();
    let mut coding = vec![0; 1228];
    coding[64] = 0x40;
    coding[65..73].copy_from_slice(&123u64.to_le_bytes());
    coding[77..79].copy_from_slice(&1u16.to_le_bytes());
    coding[83..85].copy_from_slice(&2u16.to_le_bytes());
    coding[85..87].copy_from_slice(&1u16.to_le_bytes());
    coding[89..].copy_from_slice(&shards[2]);
    for code_first in [false, true] {
        let source = udp_config(ShredDecodeMode::Ledger, 0)
            .prepare(Mode::Transaction)
            .await
            .unwrap();
        let addr = source.local_addr().unwrap().unwrap();
        let (tx, mut rx) = mpsc::channel(16);
        let handle = tokio::spawn(source.run(
            0,
            Mode::Transaction,
            Instant::now() + Duration::from_millis(150),
            tx,
        ));
        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let packets = if code_first {
            vec![&coding, &data[0]]
        } else {
            vec![&data[0], &coding]
        };
        for packet in packets {
            sender.send_to(packet, addr).await.unwrap();
        }
        let stats = handle.await.unwrap().unwrap();
        assert_eq!(
            stats.fec_recovered, 1,
            "code_first={code_first}, stats={stats:?}"
        );
        let item = rx.recv().await.unwrap();
        assert_eq!(
            item.key,
            SampleKey::Transaction {
                slot: 123,
                signature
            }
        );
        assert!(rx.recv().await.is_none());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_udp_pair_supports_all_three_modes() {
    let (bytes, _) = signed_v0_entries();
    for label in ["slot", "shred", "transaction", "one-sided"] {
        let mode = if label == "one-sided" { "shred" } else { label };
        let a = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let b = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr_a = a.local_addr().unwrap();
        let addr_b = b.local_addr().unwrap();
        drop(a);
        drop(b);
        let path = report_path(label);
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_shred-comparison"));
        command.args([
            "--mode",
            mode,
            "--shred-bind-1",
            &addr_a.to_string(),
            "--shred-bind-2",
            &addr_b.to_string(),
            "--duration-sec",
            "1",
            "--warmup-ms",
            "0",
            "--settle-window-ms",
            "100",
            "--json-output",
            path.to_str().unwrap(),
        ]);
        let (output, ready) = spawn_cli(command);
        tokio::time::timeout(Duration::from_secs(10), ready)
            .await
            .unwrap()
            .unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        for _ in 0..30 {
            for packet in packets(&bytes, 123) {
                sender.send_to(&packet, addr_a).await.unwrap();
                if label != "one-sided" {
                    sender.send_to(&packet, addr_b).await.unwrap();
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let output = output.await.unwrap();
        assert_eq!(
            output.status.success(),
            label != "one-sided",
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(report["valid"], label != "one-sided");
        if label == "one-sided" {
            assert!(report["comparison"]["sources"][0]["only"].as_u64().unwrap() > 0);
        } else {
            assert!(report["comparison"]["matched"].as_u64().unwrap() > 0);
        }
        std::fs::remove_file(path).unwrap();
    }
}

#[tokio::test]
async fn udp_benchmark_refuses_to_share_an_existing_receivers_port() {
    let owner = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )
    .unwrap();
    owner.set_reuse_address(true).unwrap();
    owner
        .bind(
            &"127.0.0.1:0"
                .parse::<std::net::SocketAddr>()
                .unwrap()
                .into(),
        )
        .unwrap();
    let address = owner.local_addr().unwrap().as_socket().unwrap();
    let mut config = udp_config(ShredDecodeMode::Ledger, 0);
    config.udp.bind_addr = address.to_string();
    assert!(
        config.prepare(Mode::Shred).await.is_err(),
        "sharing a live UDP port would bias sample coverage"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_entry_format_errors_invalidate_an_otherwise_matched_run() {
    let (bytes, _) = signed_entries();
    let (url, shutdown, server) = mock_server(vec![
        EntryMessage {
            slot: 123,
            entries: vec![1],
        },
        EntryMessage {
            slot: 123,
            entries: bytes,
        },
    ])
    .await;
    let path = report_path("invalid-entry");
    let path_clone = path.clone();
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new(env!("CARGO_BIN_EXE_shred-comparison"))
            .args([
                "--mode",
                "transaction",
                "--source-1",
                "entries-grpc",
                "--source-2",
                "entries-grpc",
                "--shred-url-1",
                &url,
                "--shred-url-2",
                &url,
                "--duration-sec",
                "1",
                "--warmup-ms",
                "0",
                "--settle-window-ms",
                "10",
                "--json-output",
                path_clone.to_str().unwrap(),
            ])
            .env("SHRED_TOKEN_1", "test-secret")
            .env("SHRED_TOKEN_2", "test-secret")
            .env("SHRED_TOKEN_HEADER_1", "x-api-key")
            .env("SHRED_TOKEN_HEADER_2", "x-api-key")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(report["valid"], false);
    assert_eq!(report["comparison"]["matched"], 1);
    assert_eq!(report["invalid_reasons"].as_array().unwrap().len(), 2);
    std::fs::remove_file(path).unwrap();
    let _ = shutdown.send(());
    server.await.unwrap();
}

fn config_command(dir: &std::path::Path) -> std::process::Command {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_shred-comparison"));
    command.current_dir(dir);
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("SHRED_") {
            command.env_remove(key);
        }
    }
    command
}

#[test]
fn config_templates_and_errors_are_validated_without_exposing_secrets() {
    let dir = report_path("config-errors");
    std::fs::create_dir(&dir).unwrap();
    for template in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/configs")).unwrap() {
        let output = config_command(&dir)
            .arg("--config")
            .arg(template.unwrap().path())
            .args(["--shred-name-1", "same", "--shred-name-2", "same"])
            .output()
            .unwrap();
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("source names must be"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let path = dir.join("bad.env");
    for (body, expected) in [
        (
            "SHRED_TOKEN_1='secret-that-must-not-leak",
            "invalid dotenv syntax",
        ),
        (
            "SHRED_DURATON_SEC=secret-that-must-not-leak",
            "unknown setting",
        ),
        (
            "SHRED_COMPARISON_DURATION_SEC=secret-that-must-not-leak",
            "invalid setting value",
        ),
        ("SHRED_MAX_KEYS=1\nSHRED_MAX_KEYS=2", "duplicate setting"),
    ] {
        std::fs::write(&path, body).unwrap();
        let output = config_command(&dir)
            .arg("--config")
            .arg(&path)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success());
        assert!(stderr.contains(expected), "{stderr}");
        assert!(!stderr.contains("secret-that-must-not-leak"));
    }
    let output = config_command(&dir)
        .args(["--config", "missing.env"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot read"));
    let help = config_command(&dir)
        .args(["--config", "missing.env", "--help"])
        .output()
        .unwrap();
    assert!(help.status.success());
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(dir).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn config_only_default_file_and_cli_env_precedence_execute_signed_entries() {
    let (bytes, _) = signed_v0_entries();
    let (url, shutdown, server) = mock_server(vec![EntryMessage {
        slot: 123,
        entries: bytes,
    }])
    .await;
    let dir = report_path("config-only");
    std::fs::create_dir(&dir).unwrap();
    let config = dir.join("shred-comparison.env");
    std::fs::write(&config, format!(
        "SHRED_COMPARISON_MODE=transaction\nSHRED_SOURCE_1=entries-grpc\nSHRED_SOURCE_2=entries-grpc\nSHRED_URL_1={url}\nSHRED_URL_2={url}\nSHRED_TOKEN_1=test-secret\nSHRED_TOKEN_2=test-secret\nSHRED_TOKEN_HEADER_1=x-api-key\nSHRED_TOKEN_HEADER_2=x-api-key\nSHRED_COMPARISON_DURATION_SEC=1\nSHRED_WARMUP_MS=0\nSHRED_SETTLE_WINDOW_MS=10\nSHRED_JSON_OUTPUT=report.json\nSHRED_NAME_1=File_A\nSHRED_NAME_2=File_B\n"
    )).unwrap();
    // No arguments: discover the default configuration in the working directory.
    // Then verify process environment > .env > file, and CLI > environment.
    for level in 0..3 {
        let mut command = config_command(&dir);
        if level > 0 {
            std::fs::write(
                dir.join(".env"),
                "SHRED_NAME_1=Dotenv_A\nSHRED_NAME_2=Dotenv_B\n",
            )
            .unwrap();
            command
                .env("SHRED_NAME_1", "Env_A")
                .env("SHRED_COMPARISON_CONFIG", &config);
            if level == 2 {
                command.args(["--shred-name-1", "Cli_A"]);
            }
        }
        let output = tokio::task::spawn_blocking(move || command.output().unwrap())
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let raw = std::fs::read_to_string(dir.join("report.json")).unwrap();
        let report: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(report["valid"], true);
        assert_eq!(report["comparison"]["matched"], 1);
        assert_eq!(
            report["comparison"]["sources"][0]["name"],
            match level {
                0 => "File_A",
                1 => "Env_A",
                _ => "Cli_A",
            }
        );
        assert_eq!(
            report["comparison"]["sources"][1]["name"],
            if level > 0 { "Dotenv_B" } else { "File_B" }
        );
        assert!(!raw.contains("test-secret"));
        assert!(!raw.contains(&url));
    }
    for file in ["shred-comparison.env", ".env", "report.json"] {
        std::fs::remove_file(dir.join(file)).unwrap();
    }
    std::fs::remove_dir(dir).unwrap();
    let _ = shutdown.send(());
    server.await.unwrap();
}
