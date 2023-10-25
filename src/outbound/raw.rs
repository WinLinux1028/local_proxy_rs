use super::ProxyOutBound;
use crate::{Connection, Error};

use tokio::{io::BufReader, net::TcpStream};

use async_trait::async_trait;

pub struct Raw();

impl Raw {
    pub fn new() -> Self {
        Raw()
    }
}

#[async_trait]
impl ProxyOutBound for Raw {
    async fn connect(&self, hostname: String, port: u16) -> Result<Connection, Error> {
        let server = TcpStream::connect(format!("{}:{}", hostname, port)).await?;
        server.set_nodelay(true)?;
        let server = tokio::io::split(server);
        let server = (BufReader::new(server.0), server.1);

        Ok(unsafe { Connection::new(Box::new(server.0), Box::new(server.1)) })
    }
}
