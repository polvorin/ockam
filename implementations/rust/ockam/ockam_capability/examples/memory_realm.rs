#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]

#[macro_use]
extern crate tracing;

use core::future::Future;
use core::time::Duration;
use ockam::compat::collections::HashMap;
use ockam::compat::sync::Mutex;
use ockam::{route, Address, Context, Message, Routed, Worker};
use std::io;

use ockam_capability::{Capabilities, Capability, UniqueUnforgeableReference};

mod echoer;
use echoer::Echoer;

// - Behaviour ----------------------------------------------------------------

#[ockam_core::async_trait]
pub trait Behaviour {
    async fn initialize(&self, ctx: &mut Context, parent: &mut Capable) -> ockam::Result<()> {
        Ok(())
    }

    async fn handle_message(
        &mut self,
        ctx: &mut Context,
        msg: Routed<CapMessage>,
        capable: &mut Capable,
    ) -> ockam::Result<()> {
        Ok(())
    }
}

// - Capable ------------------------------------------------------------------

pub struct Capable {
    name: String,
    capabilities: Capabilities,
    uur: Option<UniqueUnforgeableReference>,
}

impl Capable {
    pub fn with_capabilities(name: &str, capabilities: Capabilities) -> Self {
        let mut this = Self {
            name: name.to_string(),
            capabilities,
            uur: None,
        };
        let ptr: *const u64 = &this as *const _ as *const u64;
        let uur = UniqueUnforgeableReference(ptr as u64);
        this.uur = Some(uur);
        println!("{}'s uur is: {:?}", name, this.uur);
        this
    }

    pub fn my_capability(&self) -> Capability {
        Capability {
            name: self.name.clone(),
            uur: self.uur.unwrap(),
        }
    }

    pub fn introduce_me(&mut self, name: &'static str, capability: Capability) {
        self.capabilities.insert(name, capability);
    }

    pub fn can_delegate(&self, name: &str) -> Option<Capability> {
        if let Some(capability) = self.capabilities.get(name) {
            Some(capability.clone())
        } else {
            None
        }
    }

    pub fn approves(&self, capability: &Capability) -> bool {
        self.uur.unwrap() == capability.uur
    }
}

// - CapableWorker ------------------------------------------------------------

pub struct CapableWorker<B> {
    inner: Capable,
    behaviour: B,
}

impl<B> CapableWorker<B> {
    fn with_capabilities(name: &str, behaviour: B, capabilities: Capabilities) -> CapableWorker<B> {
        Self {
            inner: Capable::with_capabilities(name, capabilities),
            behaviour,
        }
    }

    fn my_capability(&self) -> Capability {
        self.inner.my_capability()
    }
}

#[ockam::worker]
impl<B> Worker for CapableWorker<B>
where
    B: Behaviour + Send + Sync + 'static,
{
    type Context = Context;
    type Message = CapMessage;

    async fn initialize(&mut self, ctx: &mut Self::Context) -> ockam::Result<()> {
        let inner = &mut self.inner;
        self.behaviour.initialize(ctx, inner).await
    }

    async fn handle_message(
        &mut self,
        ctx: &mut Context,
        msg: Routed<CapMessage>,
    ) -> ockam::Result<()> {
        println!(
            "\n[{}]\t[✓] Address: {}, Received: {:?}\n",
            self.inner.name,
            ctx.address(),
            msg
        );
        let inner = &mut self.inner;
        self.behaviour.handle_message(ctx, msg, inner).await
    }
}

// - CapMessage ---------------------------------------------------------------

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub enum CapMessage {
    // requests
    IntroduceRequest(Capability, String), // (Capability, "cap_name")
    OhHaiBobRequest(Capability),

    // responses
    IntroduceResponse(Capability),
    OhHaiCarolResponse,

    Unauthorized,
}

impl ockam::Message for CapMessage {}

// - ockam::node --------------------------------------------------------------

#[ockam::node]
async fn main(ctx: Context) -> ockam::Result<()> {
    // Connectivity by initial conditions: { Alice->Bob } ∵ Alice is instantiated with a reference to Bob at system initialization
    let bob = CapableWorker::with_capabilities("Bob", Bob {}, HashMap::from_iter([]));

    let alice = CapableWorker::with_capabilities(
        "Alice",
        Alice {},
        Capabilities::from_iter([("cap_bob", bob.my_capability())]),
    );

    ctx.start_worker("bob_address", bob).await?;
    ctx.start_worker("alice_address", alice).await?;

    Ok(())
}

// - Alice --------------------------------------------------------------------

struct Alice;

#[ockam_core::async_trait]
impl Behaviour for Alice {
    async fn initialize(
        &self,
        ctx: &mut Context,
        capable_alice: &mut Capable,
    ) -> ockam::Result<()> {
        // Connectivity by endowment: { Carol -> Alice } ∵ Alice instantiates Carol with a reference to their self
        let carol = CapableWorker::with_capabilities(
            "Carol",
            Carol {},
            HashMap::from_iter([("cap_alice", capable_alice.my_capability())]),
        );

        // Connectivity by parenthood: { Alice -> Carol } ∵ Alice has a reference to Carol after instantiating them
        capable_alice.introduce_me("cap_carol", carol.my_capability());

        ctx.start_worker("carol_address", carol).await?;

        Ok(())
    }

    async fn handle_message(
        &mut self,
        ctx: &mut Context,
        msg: Routed<CapMessage>,
        capable_alice: &mut Capable,
    ) -> ockam::Result<()> {
        let return_route = msg.return_route();

        if let CapMessage::IntroduceRequest(sender_capability, requested_capability_name) =
            &msg.body()
        {
            info!(
                "[Alice] Alice receives a capability request from {:?} for {:?} with {:?}",
                return_route, requested_capability_name, sender_capability
            );

            // check if Sender has a capability that allows them to talk to me
            if !capable_alice.approves(sender_capability) {
                info!("[Alice] Alice does not approve of this sender. Returning Unauthorized");
                return ctx
                    .send(return_route.clone(), CapMessage::Unauthorized)
                    .await;
            }

            if let Some(capability) = capable_alice.can_delegate(requested_capability_name.as_str())
            {
                info!(
                    "[Alice] Alice is returning the capability: {:?}",
                    capability
                );
                let cap_response = CapMessage::IntroduceResponse(capability.clone());
                return ctx.send(return_route.clone(), cap_response).await;
            }
        }

        ctx.send(return_route.clone(), CapMessage::Unauthorized)
            .await
    }
}

#[ockam::worker]
impl Worker for Alice {
    type Context = Context;
    type Message = CapMessage;
}

// - Bob ----------------------------------------------------------------------

struct Bob;

#[ockam_core::async_trait]
impl Behaviour for Bob {
    async fn initialize(&self, ctx: &mut Context, capable_bob: &mut Capable) -> ockam::Result<()> {
        Ok(())
    }

    async fn handle_message(
        &mut self,
        ctx: &mut Context,
        msg: Routed<CapMessage>,
        capable_bob: &mut Capable,
    ) -> ockam::Result<()> {
        let return_route = msg.return_route();

        if let CapMessage::OhHaiBobRequest(some_capability) = &msg.body() {
            info!("[Bob] Alice received an oh hai from {:?}", return_route);

            // check if Carol has a capability that allows them to talk to me
            if !capable_bob.approves(some_capability) {
                info!("[Bob] Bob does not approve of this sender. Returning Unauthorized");
                return ctx
                    .send(return_route.clone(), CapMessage::Unauthorized)
                    .await;
            }

            info!("[Bob] Bob sayz Oh Hai! to: {:?}", return_route);
            let cap_response = CapMessage::OhHaiCarolResponse;
            return ctx.send(return_route.clone(), cap_response).await;
        }

        Ok(())
    }
}

#[ockam::worker]
impl Worker for Bob {
    type Context = Context;
    type Message = CapMessage;
}

// - Carol --------------------------------------------------------------------

struct Carol;

#[ockam_core::async_trait]
impl Behaviour for Carol {
    async fn initialize(
        &self,
        ctx: &mut Context,
        capable_carol: &mut Capable,
    ) -> ockam::Result<()> {
        // Connectivity by invitation: { Carol -> Bob } ∵ Alice has a reference to Carol and a reference to Bob
        if let Some(cap_alice) = capable_carol.can_delegate("cap_alice") {
            println!("");
            info!("[Carol] Carol wants to use their Alice capability to ask them for Bob capability: {:?}", cap_alice);

            let cap_request =
                CapMessage::IntroduceRequest(cap_alice.clone(), "cap_bob".to_string());
            ctx.send("alice_address", cap_request).await?;

            let response = ctx.receive::<CapMessage>().await?;
            let response = response.take().body();
            println!("");
            info!(
                "[Carol] Carol got capability response from Alice: {:?}",
                response
            );

            if let CapMessage::IntroduceResponse(cap_bob) = response {
                info!("[Carol] Carol can now try saying OhHai to Bob");
                ctx.send("bob_address", CapMessage::OhHaiBobRequest(cap_bob))
                    .await?;

                let oh_hai = ctx.receive::<CapMessage>().await?;
                let oh_hai = oh_hai.take().body();
                if let CapMessage::OhHaiCarolResponse = oh_hai {
                    println!("");
                    info!("[Carol] Bob said Oh Hai to me!");
                }
            }
        }

        Ok(())
    }
}

#[ockam::worker]
impl Worker for Carol {
    type Context = Context;
    type Message = CapMessage;
}
