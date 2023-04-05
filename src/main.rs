use std::{env, io};
use std::convert::Infallible;
use std::io::ErrorKind;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use hyper::{Body, Request, Response, Server};
use hyper::server::conn::AddrIncoming;
use hyper::service::{make_service_fn, service_fn};
use surge_ping::{Client, Config, ICMP, PingIdentifier, PingSequence};
use tokio::net::lookup_host;
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tokio::time::MissedTickBehavior;
use crate::state::SharedState;
use crate::tls::{load_certs, load_private_key, TlsAcceptor};

mod color;
mod state;
mod tls;
mod uptime;

const PING_INTERVAL: Duration = Duration::from_secs(15);
const PING_TIMEOUT: Duration = Duration::from_secs(5);
const MIN_SAMPLES: u16 = 16; // 4 minutes

type SharedJoinSet = Arc<Mutex<JoinSet<()>>>;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let interface = match env::var("IFACE") {
        Ok(x) => Some(x),
        _ => None
    };

    let shared_join_set: SharedJoinSet = Arc::new(Mutex::new(JoinSet::new()));
    let shared_state: SharedState = SharedState::new();

    let mut config_v4 = Config::builder().kind(ICMP::V4);
    let mut config_v6 = Config::builder().kind(ICMP::V6);

    if let Some(interface) = interface {
        config_v4 = config_v4.interface(&interface);
        config_v6 = config_v6.interface(&interface);
    }

    let client_v4 = Client::new(&config_v4.build()).unwrap();
    let client_v6 = Client::new(&config_v6.build()).unwrap();

    let make_service = make_service_fn(move |_conn| {
        let shared_join_set = shared_join_set.clone();
        let shared_state = shared_state.clone();
        let client_v4 = client_v4.clone();
        let client_v6 = client_v6.clone();

        let service = service_fn(move |req|
            handle(shared_join_set.clone(), shared_state.clone(), client_v4.clone(), client_v6.clone(), req)
        );

        async move { Ok::<_, Infallible>(service) }
    });

    let tls_cfg = {
        let certs = load_certs("/etc/uptimeem/ssl.pem").unwrap();
        let key = load_private_key("/etc/uptimeem/ssl.key").unwrap();

        let mut cfg = rustls::ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| io::Error::new(ErrorKind::Other, format!("{}", e))).unwrap();

        cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec(), b"http/1.0".to_vec()];
        Arc::new(cfg)
    };

    let addr = SocketAddr::from(([0, 0, 0, 0], 443));
    let server = Server::builder(TlsAcceptor::new(tls_cfg, AddrIncoming::bind(&addr).unwrap())).serve(make_service);

    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}

async fn handle(
    shared_join_set: SharedJoinSet,
    shared_state: SharedState,
    client_v4: Client,
    client_v6: Client,
    req: Request<Body>,
) -> Result<Response<Body>, Infallible> {
        let (representation, ip_str) = match req.uri().path().trim_end_matches("/").split("/").skip(1).collect::<Vec<&str>>()[..] {
        ["num_tracked"] => {
            return Ok(Response::new(Body::from(format!("{{\
                \"schemaVersion\": 1,\
                \"label\": \"tracked hosts\",\
                \"message\": \"{}\",\
                \"color\": \"blue\"\
                }}", shared_state.get_num_tracked()
            ))))
        },
        ["version"] => {
            return Ok(Response::new(Body::from(format!("{{\
            \"schemaVersion\": 1,\
            \"label\": \"hosted version\",\
            \"message\": \"{}\",\
            \"color\": \"blue\"\
            }}", env!("CARGO_PKG_VERSION")
            ))))
        },
        [target, "by_avg"] if target.len() > 0 && target.len() <= 253 => ("by_avg", target),
        [target, "by_loss"] if target.len() > 0 && target.len() <= 253 => ("by_loss", target),
        _ => {
            return Ok(Response::new(Body::from("{\
            \"schemaVersion\": 1,\
            \"label\": \"uptime\",\
            \"message\": \"invalid parameters\",\
            \"color\": \"critical\",\
            \"isError\": true\
            }"
            )))
        }
    };

    let ip_addr = match lookup_host(format!("{}:0", ip_str))
        .await
        .map(|mut x| x.next().map(|x| x.ip()))
    {
        Ok(Some(x)) => x,
        _ => return Ok(Response::new(Body::from("{\
            \"schemaVersion\": 1,\
            \"label\": \"uptime\",\
            \"message\": \"unresolvable hostname\",\
            \"color\": \"critical\",\
            \"isError\": true\
            }"
        )))
    };

    let uptime_str = match representation {
        "by_avg" => shared_state.get_uptime_by_avg(&ip_addr).await,
        "by_loss" => shared_state.get_uptime_by_loss(&ip_addr).await,
        _ => unreachable!()
    };

    match uptime_str {
        Some((percentage, color)) => {
            return Ok(Response::new(Body::from(format!("{{\
                \"schemaVersion\": 1,\
                \"label\": \"uptime\",\
                \"message\": \"{}\",\
                \"color\": \"{}\"\
                }}", percentage, color
            ))))
        },
        None => {
            shared_state.insert(ip_addr.clone()).await;

            if ip_addr.is_ipv4() {
                shared_join_set.lock().await.spawn(ping_loop(shared_state.clone(), client_v4.clone(), ip_addr));
            } else if ip_addr.is_ipv6() {
                shared_join_set.lock().await.spawn(ping_loop(shared_state.clone(), client_v6.clone(), ip_addr));
            }

            return Ok(Response::new(Body::from("{\
                \"schemaVersion\": 1,\
                \"label\": \"uptime\",\
                \"message\": \"??%\",\
                \"color\": \"lightgrey\"\
                }"
            )))
        }
    }
}

// continuously ping given address
async fn ping_loop(shared_state: SharedState, client: Client, ip: IpAddr) {
    let mut interval = tokio::time::interval(PING_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let ping_identifier = PingIdentifier(rand::random());
    let mut n_sequence = 0;

    let mut pinger = client.pinger(ip, ping_identifier).await;
    pinger.timeout(PING_TIMEOUT);

    loop {
        interval.tick().await;

        let ping_sequence = PingSequence(n_sequence);

        match pinger.ping(ping_sequence, &[]).await {
            Ok((packet, _rtt)) if packet.get_identifier() == ping_identifier && packet.get_sequence() == ping_sequence => {
                if !shared_state.succ_ping(&ip).await {
                    break
                }
            },
            _ => {
                if !shared_state.fail_ping(&ip).await {
                    break
                }
            }
        }

        if n_sequence == u16::MAX {
            n_sequence = 0;
        } else {
            n_sequence += 1;
        }
    }
}