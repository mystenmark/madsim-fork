use std::pin::Pin;
use std::time::Duration;

use async_stream::try_stream;
use futures_core::Stream;
use madsim::time::sleep;
use tonic::{transport::Server, Request, Response, Status, Streaming};

use hello_world::another_greeter_server::{AnotherGreeter, AnotherGreeterServer};
use hello_world::greeter_server::{Greeter, GreeterServer};
use hello_world::{HelloReply, HelloRequest};

pub mod hello_world {
    tonic::include_proto!("helloworld");
}

#[derive(Debug, Default)]
pub struct MyGreeter {}

#[tonic::async_trait]
impl AnotherGreeter for MyGreeter {
    async fn say_hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloReply>, Status> {
        println!("Got a request: {:?}", request);
        let reply = HelloReply {
            message: format!("Hi {}!", request.into_inner().name),
        };
        Ok(Response::new(reply))
    }
}

#[tonic::async_trait]
impl Greeter for MyGreeter {
    async fn say_hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloReply>, Status> {
        println!("Got a request: {:?}", request);
        let reply = HelloReply {
            message: format!("Hello {}!", request.into_inner().name),
        };
        Ok(Response::new(reply))
    }

    type LotsOfRepliesStream = Pin<Box<dyn Stream<Item = Result<HelloReply, Status>> + Send>>;

    async fn lots_of_replies(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<Self::LotsOfRepliesStream>, Status> {
        println!("Got a request: {:?}", request);
        let stream = try_stream! {
            let name = request.into_inner().name;
            for i in 0..3 {
                yield HelloReply {
                    message: format!("{i}: Hello {name}!"),
                };
                sleep(Duration::from_secs(1)).await;
            }
        };
        Ok(Response::new(Box::pin(stream)))
    }

    async fn lots_of_greetings(
        &self,
        request: Request<Streaming<HelloRequest>>,
    ) -> Result<Response<HelloReply>, Status> {
        println!("Got a request: {:?}", request);
        let mut stream = request.into_inner();
        let mut s = String::new();
        while let Some(request) = stream.message().await? {
            println!("-> {:?}", request);
            s += " ";
            s += &request.name;
        }
        let reply = HelloReply {
            message: format!("Hello{s}!"),
        };
        Ok(Response::new(reply))
    }

    type BidiHelloStream = Pin<Box<dyn Stream<Item = Result<HelloReply, Status>> + Send>>;

    async fn bidi_hello(
        &self,
        request: Request<Streaming<HelloRequest>>,
    ) -> Result<Response<Self::BidiHelloStream>, Status> {
        println!("Got a request: {:?}", request);
        let stream = try_stream! {
            let mut stream = request.into_inner();
            while let Some(request) = stream.message().await? {
                println!("-> {:?}", request);
                yield HelloReply {
                    message: format!("Hello {}!", request.name),
                };
            }
        };
        Ok(Response::new(Box::pin(stream)))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse()?;

    Server::builder()
        .add_service(GreeterServer::new(MyGreeter::default()))
        .add_service(AnotherGreeterServer::new(MyGreeter::default()))
        .serve(addr)
        .await?;

    Ok(())
}

#[cfg(test)]
#[cfg(madsim)]
mod tests {
    use super::hello_world::another_greeter_client::AnotherGreeterClient;
    use super::hello_world::greeter_client::GreeterClient;
    use async_stream::stream;
    use madsim::{runtime::Handle, time::sleep};
    use std::net::SocketAddr;

    use super::*;

    #[madsim::test]
    async fn test() {
        let handle = Handle::current();
        let addr0 = "10.0.0.1:50051".parse::<SocketAddr>().unwrap();
        let ip1 = "10.0.0.2".parse().unwrap();
        let ip2 = "10.0.0.3".parse().unwrap();
        let ip3 = "10.0.0.4".parse().unwrap();
        let ip4 = "10.0.0.5".parse().unwrap();
        let ip5 = "10.0.0.6".parse().unwrap();
        let node0 = handle.create_node().name("server").ip(addr0.ip()).build();
        let node1 = handle.create_node().name("client1").ip(ip1).build();
        let node2 = handle.create_node().name("client2").ip(ip2).build();
        let node3 = handle.create_node().name("client3").ip(ip3).build();
        let node4 = handle.create_node().name("client4").ip(ip4).build();
        let node5 = handle.create_node().name("client5").ip(ip5).build();

        node0.spawn(async move {
            Server::builder()
                .add_service(GreeterServer::new(MyGreeter::default()))
                .add_service(AnotherGreeterServer::new(MyGreeter::default()))
                .serve(addr0)
                .await
                .unwrap();
        });

        // unary
        let task1 = node1.spawn(async move {
            sleep(Duration::from_secs(1)).await;
            let mut client = GreeterClient::connect("http://10.0.0.1:50051")
                .await
                .unwrap();
            let request = tonic::Request::new(HelloRequest {
                name: "Tonic".into(),
            });
            let response = client.say_hello(request).await.unwrap();
            assert_eq!(response.into_inner().message, "Hello Tonic!");
        });

        // another service
        let task2 = node2.spawn(async move {
            sleep(Duration::from_secs(1)).await;
            let mut client = AnotherGreeterClient::connect("http://10.0.0.1:50051")
                .await
                .unwrap();
            let request = tonic::Request::new(HelloRequest {
                name: "Tonic".into(),
            });
            let response = client.say_hello(request).await.unwrap();
            assert_eq!(response.into_inner().message, "Hi Tonic!");
        });

        // server stream
        let task3 = node3.spawn(async move {
            sleep(Duration::from_secs(1)).await;
            let mut client = GreeterClient::connect("http://10.0.0.1:50051")
                .await
                .unwrap();
            let request = tonic::Request::new(HelloRequest {
                name: "Tonic".into(),
            });
            let response = client.lots_of_replies(request).await.unwrap();
            let mut stream = response.into_inner();
            let mut i = 0;
            while let Some(reply) = stream.message().await.unwrap() {
                assert_eq!(reply.message, format!("{i}: Hello Tonic!"));
                i += 1;
            }
            assert_eq!(i, 3);
        });

        let new_stream = || {
            stream! {
                for i in 0..3 {
                    yield HelloRequest {
                        name: format!("Tonic{i}"),
                    };
                    sleep(Duration::from_secs(1)).await;
                }
            }
        };

        // client stream
        let task4 = node4.spawn(async move {
            sleep(Duration::from_secs(1)).await;
            let mut client = GreeterClient::connect("http://10.0.0.1:50051")
                .await
                .unwrap();
            let response = client.lots_of_greetings(new_stream()).await.unwrap();
            assert_eq!(response.into_inner().message, "Hello Tonic0 Tonic1 Tonic2!");
        });

        // bi-directional stream
        let task5 = node5.spawn(async move {
            sleep(Duration::from_secs(1)).await;
            let mut client = GreeterClient::connect("http://10.0.0.1:50051")
                .await
                .unwrap();
            let response = client.bidi_hello(new_stream()).await.unwrap();
            let mut stream = response.into_inner();
            let mut i = 0;
            while let Some(reply) = stream.message().await.unwrap() {
                assert_eq!(reply.message, format!("Hello Tonic{i}!"));
                i += 1;
            }
            assert_eq!(i, 3);
        });

        task1.await.unwrap();
        task2.await.unwrap();
        task3.await.unwrap();
        task4.await.unwrap();
        task5.await.unwrap();
    }
}
