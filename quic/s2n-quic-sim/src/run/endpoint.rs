// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::{events, CliRange};
use bytes::Bytes;
use s2n_quic::{
    client::Connect,
    provider::{
        event::tracing::Subscriber as Tracing,
        io::testing::{primary, rand, spawn, time, Handle, Result},
    },
    Client, Server,
};
use s2n_quic_core::{crypto::tls::testing::certificates, stream::testing::Data};
use std::net::SocketAddr;

pub fn server(handle: &Handle, events: events::Events) -> Result<SocketAddr> {
    let server = Server::builder()
        .with_io(handle.builder().build().unwrap())?
        .with_tls((certificates::CERT_PEM, certificates::KEY_PEM))?
        .with_limits(Limits::default().limits())?
        .with_event((events, Tracing::default()))?;

    let mut server = if std::env::var("BBR").is_ok() {
        server
            .with_congestion_controller(s2n_quic::provider::congestion_controller::Bbr::default())?
            .start()?
    } else {
        server.start()?
    };

    let server_addr = server.local_addr()?;

    // accept connections and echo back
    spawn(async move {
        while let Some(mut connection) = server.accept().await {
            primary::spawn(async move {
                while let Ok(Some(mut stream)) = connection.accept_bidirectional_stream().await {
                    primary::spawn(async move {
                        let mut send_size = None;

                        while let Ok(Some(chunk)) = stream.receive().await {
                            if send_size.is_none() {
                                send_size =
                                    Some(u64::from_be_bytes(chunk[..8].try_into().unwrap()));
                            }
                            let _ = chunk;
                        }

                        let mut data = Data::new(send_size.unwrap_or_default());

                        while let Some(chunk) = data.send_one(usize::MAX) {
                            if stream.send(chunk).await.is_err() {
                                break;
                            }
                        }

                        let _ = stream.finish();
                        let _ = stream.flush().await;
                    });
                }
            });
        }
    });

    Ok(server_addr)
}

pub fn client(
    handle: &Handle,
    events: events::Events,
    servers: &[SocketAddr],
    count: usize,
    delay: CliRange<humantime::Duration>,
    stream_count: u32,
    stream_data: CliRange<u64>,
) -> Result {
    let client = Client::builder()
        .with_io(handle.builder().build().unwrap())?
        .with_tls(certificates::CERT_PEM)?
        .with_limits(Limits::default().limits())?
        .with_event((events, Tracing::default()))?
        .start()?;

    for _ in 0..count {
        let conn_delay = delay.gen_duration();

        // pick a random server to connect to
        let server_addr = *rand::one_of(servers);

        let connect = Connect::new(server_addr).with_server_name("localhost");
        let connection = client.connect(connect);
        primary::spawn(async move {
            if !conn_delay.is_zero() {
                time::delay(conn_delay).await;
            }

            let connection = connection.await?;

            let mut stream_count = stream_count;

            while stream_count > 0 {
                let stream_delay = delay.gen_duration();
                if !stream_delay.is_zero() {
                    time::delay(stream_delay).await;
                }
                let stream_burst = rand::gen_range(1..=stream_count);
                stream_count -= stream_burst;

                for _ in 0..stream_burst {
                    let mut handle = connection.handle();

                    primary::spawn(async move {
                        let stream = handle.open_bidirectional_stream().await?;

                        let (mut recv, mut send) = stream.split();

                        let response_size = stream_data.gen();
                        send.send(Bytes::copy_from_slice(&response_size.to_be_bytes()))
                            .await?;

                        let mut send_data = Data::new(160);

                        while let Some(chunk) = send_data.send_one(usize::MAX) {
                            send.send(chunk).await?;
                        }
                        send.finish()?;

                        while recv.receive().await?.is_some() {}

                        <s2n_quic::stream::Result<()>>::Ok(())
                    });
                }
            }

            <s2n_quic::stream::Result<()>>::Ok(())
        });
    }

    Ok(())
}

#[derive(Debug)]
pub struct Limits {
    /// The maximum bits/sec for each connection
    pub max_throughput: u64,

    /// The expected RTT in milliseconds
    pub expected_rtt: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_throughput: 10_000,
            expected_rtt: 100,
        }
    }
}

impl Limits {
    pub fn limits(&self) -> s2n_quic::provider::limits::Limits {
        let data_window = self.data_window();

        s2n_quic::provider::limits::Limits::default()
            .with_data_window(data_window)
            .unwrap()
            .with_max_send_buffer_size(data_window.min(u32::MAX as _) as _)
            .unwrap()
            .with_bidirectional_local_data_window(data_window)
            .unwrap()
            .with_bidirectional_remote_data_window(data_window)
            .unwrap()
            .with_unidirectional_data_window(data_window)
            .unwrap()
            .with_max_open_local_bidirectional_streams(u32::MAX as _)
            .unwrap()
            .with_max_open_remote_bidirectional_streams(u32::MAX as _)
            .unwrap()
    }

    fn data_window(&self) -> u64 {
        s2n_quic_core::transport::parameters::compute_data_window(
            self.max_throughput,
            core::time::Duration::from_millis(self.expected_rtt),
            2,
        )
        .as_u64()
    }
}
