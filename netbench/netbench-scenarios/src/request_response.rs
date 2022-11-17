// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use netbench_scenarios::prelude::*;

config!({
    /// The size of the client's request to the server
    let request_size: Byte = 1.kilobytes();

    /// The size of the server's response to the client
    let response_size: Byte = 10.megabytes();

    /// How long the client will wait before making another request
    let request_delay: Duration = 0.seconds();

    /// How long the server will take to respond to the request
    let response_delay: Duration = 0.seconds();

    /// The number of requests to make
    let count: u64 = 1;

    /// The number of separate connections to create
    let connections: u64 = 1;

    /// How long the client will wait before establishing another connetion
    let connect_delay: Duration = 0.seconds();

    /// Specifies if the requests should be performed in parallel
    let parallel: bool = false;

    /// Specifies if the connections should be opened in parallel
    let parallel_connections: bool = false;

    /// The rate at which the client sends data
    let client_send_rate: Option<Rate> = None;

    /// The rate at which the client receives data
    let client_receive_rate: Option<Rate> = None;

    /// The rate at which the server sends data
    let server_send_rate: Option<Rate> = None;

    /// The rate at which the server receives data
    let server_receive_rate: Option<Rate> = None;

    /// The number of bytes that must be received before the next request
    let response_unblock: Byte = 0.bytes();
});

pub fn scenario(config: Config) -> Scenario {
    let Config {
        request_size,
        response_size,
        count,
        connections,
        connect_delay,
        parallel,
        parallel_connections,
        client_send_rate,
        client_receive_rate,
        server_send_rate,
        server_receive_rate,
        request_delay,
        response_delay,
        response_unblock,
    } = config;
    let response_unblock = response_unblock.min(response_size);

    type Checkpoint = Option<
        builder::checkpoint::Checkpoint<builder::Client, builder::Local, builder::checkpoint::Park>,
    >;

    let request = |conn: &mut builder::connection::Builder<builder::Client>,
                   checkpoint: &mut Checkpoint| {
        let (park, unpark) = conn.checkpoint();

        if let Some(park) = checkpoint.take() {
            conn.park(park);
        }

        conn.open_bidirectional_stream(
            |local| {
                if let Some(rate) = client_send_rate {
                    local.set_send_rate(rate);
                }
                if let Some(rate) = client_receive_rate {
                    local.set_receive_rate(rate);
                }
                local.send(request_size);

                if *response_unblock > 0 {
                    local.receive(response_unblock);
                    local.unpark(unpark);
                    local.receive(response_size - response_unblock);
                } else {
                    local.receive(response_size);
                }
            },
            |remote| {
                if let Some(rate) = server_send_rate {
                    remote.set_send_rate(rate);
                }
                if let Some(rate) = server_receive_rate {
                    remote.set_receive_rate(rate);
                }
                remote.receive(request_size);

                if response_delay != Duration::ZERO {
                    remote.sleep(response_delay);
                }

                remote.send(response_size);
            },
        );

        if *response_unblock > 0 {
            *checkpoint = Some(park)
        }
    };

    Scenario::build(|scenario| {
        let server = scenario.create_server();

        let connect_to_server = |client: &mut builder::client::Builder| {
            client.connect_to(&server, |conn| {
                if parallel {
                    conn.scope(|scope| {
                        let mut prev_checkpoint = None;
                        for idx in 0..count {
                            scope.spawn(|conn| {
                                let request_delay = request_delay * idx as u32;
                                if request_delay != Duration::ZERO {
                                    conn.sleep(request_delay);
                                }

                                request(conn, &mut prev_checkpoint);
                            });
                        }
                    });
                } else {
                    for _ in 0..count {
                        request(conn, &mut None);

                        if request_delay != Duration::ZERO {
                            conn.sleep(request_delay);
                        }
                    }
                }
            });
        };

        scenario.create_client(|client| {
            if parallel_connections {
                client.scope(|scope| {
                    for idx in 0..connections {
                        scope.spawn(|client| {
                            let connect_delay = connect_delay * idx as u32;
                            if connect_delay != Duration::ZERO {
                                client.sleep(connect_delay);
                            }

                            connect_to_server(client);
                        });
                    }
                });
            } else {
                for _ in 0..connections {
                    connect_to_server(client);

                    if connect_delay != Duration::ZERO {
                        client.sleep(connect_delay);
                    }
                }
            }
        });
    })
}
