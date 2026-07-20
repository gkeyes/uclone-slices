use crate::protocol::{Command, ProtocolError, Request, Response};

use super::Transport;

pub(super) struct RescueFallback<'a, T, D> {
    transport: &'a mut T,
    direct: D,
}

impl<'a, T, D> RescueFallback<'a, T, D> {
    pub(super) const fn new(transport: &'a mut T, direct: D) -> Self {
        Self { transport, direct }
    }
}

impl<T, D> Transport for RescueFallback<'_, T, D>
where
    T: Transport,
    D: FnMut(&Request) -> Response,
{
    fn request(&mut self, request: &Request) -> Result<Response, ProtocolError> {
        match self.transport.request(request) {
            Ok(response) => Ok(response),
            Err(_) if matches!(request.command(), Command::RescueToBase { .. }) => {
                Ok((self.direct)(request))
            }
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use crate::domain::PackageName;
    use crate::protocol::{
        ALLOWED_PACKAGE, Ack, AckOperation, Command, Request, RequestId, ResponsePayload,
        ResponseStatus,
    };

    use super::*;

    #[derive(Debug)]
    struct FailedTransport;

    impl Transport for FailedTransport {
        fn request(&mut self, _request: &Request) -> Result<Response, ProtocolError> {
            Err(io::Error::from(io::ErrorKind::TimedOut).into())
        }
    }

    fn request(command: Command) -> Request {
        Request::new(RequestId::new("fallback-test").unwrap(), command).unwrap()
    }

    #[test]
    fn rescue_transport_failure_uses_direct_path() {
        let package = PackageName::parse(ALLOWED_PACKAGE).unwrap();
        let request = request(Command::RescueToBase { package });
        let mut daemon = FailedTransport;
        let mut fallback = RescueFallback::new(&mut daemon, |request: &Request| {
            Response::ok(
                request.request_id().clone(),
                ResponsePayload::Ack(Ack::new(AckOperation::RescueToBase)),
            )
            .unwrap()
        });
        assert_eq!(
            fallback.request(&request).unwrap().status(),
            ResponseStatus::Ok
        );
    }

    #[test]
    fn ordinary_transport_failure_does_not_use_direct_path() {
        let request = request(Command::Probe);
        let mut daemon = FailedTransport;
        let mut fallback = RescueFallback::new(&mut daemon, |_request: &Request| unreachable!());
        assert!(fallback.request(&request).is_err());
    }
}
