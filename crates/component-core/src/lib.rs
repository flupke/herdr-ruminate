use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::marker::PhantomData;
use std::rc::{Rc, Weak};
use std::sync::Arc;

const MAX_EVENTS_PER_PUBLICATION: usize = 1_024;

pub trait ApplicationEvent: Any + Send + Sync + 'static {}

impl<T> ApplicationEvent for T where T: Any + Send + Sync + 'static {}

#[derive(Clone)]
pub struct EventEnvelope {
    value: Arc<dyn Any + Send + Sync>,
    created_at: std::time::Instant,
    type_name: &'static str,
}

impl EventEnvelope {
    pub fn new<E>(event: E) -> Self
    where
        E: ApplicationEvent,
    {
        Self {
            value: Arc::new(event),
            created_at: std::time::Instant::now(),
            type_name: std::any::type_name::<E>(),
        }
    }

    pub fn created_at(&self) -> std::time::Instant {
        self.created_at
    }

    pub fn type_name(&self) -> &'static str {
        self.type_name
    }

    pub fn downcast_ref<E>(&self) -> Option<&E>
    where
        E: ApplicationEvent,
    {
        self.value.downcast_ref()
    }
}

impl std::fmt::Debug for EventEnvelope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EventEnvelope")
            .field("event_type", &self.value.as_ref().type_id())
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub struct EventPublisher {
    pending_events: Weak<RefCell<VecDeque<EventEnvelope>>>,
}

impl EventPublisher {
    pub fn publish<E>(&self, event: E)
    where
        E: ApplicationEvent,
    {
        if let Some(pending_events) = self.pending_events.upgrade() {
            pending_events
                .borrow_mut()
                .push_back(EventEnvelope::new(event));
        }
    }
}

impl std::fmt::Debug for EventPublisher {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EventPublisher")
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub struct ApplicationEventSender(crossbeam_channel::Sender<EventEnvelope>);

impl ApplicationEventSender {
    pub fn new(sender: crossbeam_channel::Sender<EventEnvelope>) -> Self {
        Self(sender)
    }

    pub fn send<E>(&self, event: E) -> Result<(), crossbeam_channel::SendError<EventEnvelope>>
    where
        E: ApplicationEvent,
    {
        self.send_envelope(EventEnvelope::new(event))
    }

    pub fn send_envelope(
        &self,
        event: EventEnvelope,
    ) -> Result<(), crossbeam_channel::SendError<EventEnvelope>> {
        self.0.send(event)
    }
}

impl std::fmt::Debug for ApplicationEventSender {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ApplicationEventSender")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct DispatchResult<A> {
    actions: Vec<A>,
}

impl<A> DispatchResult<A> {
    pub fn into_actions(self) -> Vec<A> {
        self.actions
    }
}

pub trait IntoDispatchResult<A> {
    fn into_dispatch_result(self) -> DispatchResult<A>;
}

impl<A> IntoDispatchResult<A> for () {
    fn into_dispatch_result(self) -> DispatchResult<A> {
        DispatchResult {
            actions: Vec::new(),
        }
    }
}

impl<Action> IntoDispatchResult<Action> for Vec<Action> {
    fn into_dispatch_result(self) -> DispatchResult<Action> {
        DispatchResult { actions: self }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum InputResolution<T> {
    NoMatch,
    AwaitingMoreInput,
    Matched(T),
}

impl<T> InputResolution<T> {
    pub fn map<U>(self, convert: impl FnOnce(T) -> U) -> InputResolution<U> {
        match self {
            Self::NoMatch => InputResolution::NoMatch,
            Self::AwaitingMoreInput => InputResolution::AwaitingMoreInput,
            Self::Matched(value) => InputResolution::Matched(convert(value)),
        }
    }
}

pub trait InputMatcher<C, E>: 'static {
    type Output: ApplicationEvent;

    fn resolve(&mut self, component: &C, input: &E) -> InputResolution<Self::Output>;
}

pub struct AnyInput;

impl<C, E> InputMatcher<C, E> for AnyInput
where
    E: ApplicationEvent + Clone,
{
    type Output = E;

    fn resolve(&mut self, _component: &C, input: &E) -> InputResolution<Self::Output> {
        InputResolution::Matched(input.clone())
    }
}

pub trait Component<A>: Sized + 'static
where
    A: Send + 'static,
{
    fn register_subscriptions(subscriptions: &mut ComponentSubscriptions<'_, Self, A>);
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ComponentTarget(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputScope {
    Global,
    Focused,
    Enclosing,
    Hovered,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SubscriptionKind {
    Event,
    Input(InputScope),
}

type HandlerInvocation<A> = InputResolution<DispatchResult<A>>;

type ErasedHandler<A> =
    Rc<dyn Fn(&mut dyn Any, &dyn Any) -> Result<HandlerInvocation<A>, DispatchError>>;

struct Subscription<A> {
    component_id: ComponentTarget,
    kind: SubscriptionKind,
    handler: ErasedHandler<A>,
}

impl<A> Clone for Subscription<A> {
    fn clone(&self) -> Self {
        Self {
            component_id: self.component_id,
            kind: self.kind,
            handler: Rc::clone(&self.handler),
        }
    }
}

pub struct ComponentSubscriptions<'a, C, A> {
    component_id: ComponentTarget,
    subscriptions_by_event: &'a mut HashMap<TypeId, Vec<Subscription<A>>>,
    component: PhantomData<C>,
}

impl<C, A> ComponentSubscriptions<'_, C, A>
where
    C: Component<A>,
    A: Send + 'static,
{
    pub fn subscribe<E, R>(&mut self, handler: fn(&mut C, &E) -> R)
    where
        E: ApplicationEvent,
        R: IntoDispatchResult<A> + 'static,
    {
        let erased_handler: ErasedHandler<A> = Rc::new(move |component, event| {
            let component = component
                .downcast_mut::<C>()
                .ok_or(DispatchError::ComponentTypeMismatch)?;
            let event = event
                .downcast_ref::<E>()
                .ok_or(DispatchError::EventTypeMismatch)?;
            Ok(HandlerInvocation::Matched(
                handler(component, event).into_dispatch_result(),
            ))
        });
        let subscriptions = self
            .subscriptions_by_event
            .entry(TypeId::of::<E>())
            .or_default();
        subscriptions.push(Subscription {
            component_id: self.component_id,
            kind: SubscriptionKind::Event,
            handler: erased_handler,
        });
    }

    pub fn subscribe_input<E, M, R>(
        &mut self,
        scope: InputScope,
        matcher: M,
        handler: fn(&mut C, M::Output) -> R,
    ) where
        E: ApplicationEvent,
        M: InputMatcher<C, E>,
        R: IntoDispatchResult<A> + 'static,
    {
        let matcher = RefCell::new(matcher);
        let erased_handler: ErasedHandler<A> = Rc::new(move |component, event| {
            let component = component
                .downcast_mut::<C>()
                .ok_or(DispatchError::ComponentTypeMismatch)?;
            let event = event
                .downcast_ref::<E>()
                .ok_or(DispatchError::EventTypeMismatch)?;
            let resolution = matcher.borrow_mut().resolve(component, event);
            match resolution {
                InputResolution::NoMatch => Ok(HandlerInvocation::NoMatch),
                InputResolution::AwaitingMoreInput => Ok(HandlerInvocation::AwaitingMoreInput),
                InputResolution::Matched(input) => Ok(HandlerInvocation::Matched(
                    handler(component, input).into_dispatch_result(),
                )),
            }
        });
        self.subscriptions_by_event
            .entry(TypeId::of::<E>())
            .or_default()
            .push(Subscription {
                component_id: self.component_id,
                kind: SubscriptionKind::Input(scope),
                handler: erased_handler,
            });
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchError {
    ComponentNotFound,
    ComponentTypeMismatch,
    EventTypeMismatch,
    EventCycleLimitExceeded,
}

pub struct InputDispatch<A> {
    results: Vec<DispatchResult<A>>,
    input_pending: bool,
}

impl<A> InputDispatch<A> {
    pub fn into_results(self) -> Vec<DispatchResult<A>> {
        self.results
    }

    pub fn global_input_pending(&self) -> bool {
        self.input_pending
    }
}

pub struct ComponentEventBus<A: Send + 'static> {
    next_component_id: u64,
    pending_events: Rc<RefCell<VecDeque<EventEnvelope>>>,
    subscriptions_by_event: HashMap<TypeId, Vec<Subscription<A>>>,
    components: HashMap<ComponentTarget, Box<dyn Any>>,
}

impl<A: Send + 'static> Default for ComponentEventBus<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: Send + 'static> ComponentEventBus<A> {
    pub fn new() -> Self {
        Self {
            next_component_id: 0,
            pending_events: Rc::new(RefCell::new(VecDeque::new())),
            subscriptions_by_event: HashMap::new(),
            components: HashMap::new(),
        }
    }

    /// # Panics
    ///
    /// Panics if the process mounts more than `u64::MAX` components through
    /// this bus.
    pub fn mount<C>(&mut self, construct: impl FnOnce(EventPublisher) -> C) -> ComponentTarget
    where
        C: Component<A>,
    {
        let component_id = ComponentTarget(self.next_component_id);
        self.next_component_id = self
            .next_component_id
            .checked_add(1)
            .expect("component identifiers must not overflow");
        let publisher = EventPublisher {
            pending_events: Rc::downgrade(&self.pending_events),
        };
        let component = construct(publisher);
        C::register_subscriptions(&mut ComponentSubscriptions {
            component_id,
            subscriptions_by_event: &mut self.subscriptions_by_event,
            component: PhantomData,
        });
        let replaced = self.components.insert(component_id, Box::new(component));
        debug_assert!(replaced.is_none());
        component_id
    }

    pub fn remove(&mut self, target: ComponentTarget) -> bool {
        let removed = self.components.remove(&target).is_some();
        if removed {
            self.subscriptions_by_event.retain(|_, subscriptions| {
                subscriptions.retain(|subscription| subscription.component_id != target);
                !subscriptions.is_empty()
            });
        }
        removed
    }

    pub fn get<C>(&self, target: ComponentTarget) -> Option<&C>
    where
        C: Component<A>,
    {
        self.components.get(&target)?.downcast_ref::<C>()
    }

    pub fn get_mut<C>(&mut self, target: ComponentTarget) -> Option<&mut C>
    where
        C: Component<A>,
    {
        self.components.get_mut(&target)?.downcast_mut::<C>()
    }

    pub fn publish<E>(&mut self, event: E) -> Result<Vec<DispatchResult<A>>, DispatchError>
    where
        E: ApplicationEvent,
    {
        self.publish_envelope(EventEnvelope::new(event))
    }

    pub fn publish_envelope(
        &mut self,
        event: EventEnvelope,
    ) -> Result<Vec<DispatchResult<A>>, DispatchError> {
        self.pending_events.borrow_mut().push_back(event);
        self.deliver_pending_events()
    }

    pub fn dispatch_input(
        &mut self,
        event: &EventEnvelope,
        focused_target: ComponentTarget,
    ) -> Result<InputDispatch<A>, DispatchError> {
        let mut results = Vec::new();
        let mut focused_matched = false;
        let subscriptions =
            self.matching_subscriptions(event.value.as_ref().type_id(), |subscription| {
                subscription.kind == SubscriptionKind::Input(InputScope::Focused)
                    && subscription.component_id == focused_target
            });
        for subscription in subscriptions {
            match self.invoke(&subscription, event)? {
                HandlerInvocation::NoMatch => {}
                HandlerInvocation::AwaitingMoreInput => focused_matched = true,
                HandlerInvocation::Matched(result) => {
                    focused_matched = true;
                    results.push(result);
                }
            }
        }
        let mut global_input_pending = false;
        if !focused_matched {
            let global = self.dispatch_global_subscriptions(event)?;
            global_input_pending = global.input_pending;
            results.extend(global.results);
        }
        results.extend(self.deliver_pending_events()?);
        Ok(InputDispatch {
            results,
            input_pending: global_input_pending,
        })
    }

    pub fn dispatch_enclosing_input(
        &mut self,
        event: &EventEnvelope,
        focused_target: ComponentTarget,
    ) -> Result<Option<Vec<DispatchResult<A>>>, DispatchError> {
        let subscriptions =
            self.matching_subscriptions(event.value.as_ref().type_id(), |subscription| {
                subscription.kind == SubscriptionKind::Input(InputScope::Enclosing)
                    && subscription.component_id != focused_target
            });
        let mut dispatch = self.invoke_input(subscriptions, event)?;
        if dispatch.results.is_empty() && !dispatch.input_pending {
            return Ok(None);
        }
        dispatch.results.extend(self.deliver_pending_events()?);
        Ok(Some(dispatch.results))
    }

    pub fn dispatch_global_input(
        &mut self,
        event: &EventEnvelope,
    ) -> Result<InputDispatch<A>, DispatchError> {
        let mut dispatch = self.dispatch_global_subscriptions(event)?;
        dispatch.results.extend(self.deliver_pending_events()?);
        Ok(dispatch)
    }

    pub fn dispatch_hovered_input(
        &mut self,
        event: &EventEnvelope,
        target: ComponentTarget,
    ) -> Result<InputDispatch<A>, DispatchError> {
        let subscriptions =
            self.matching_subscriptions(event.value.as_ref().type_id(), |subscription| {
                subscription.kind == SubscriptionKind::Input(InputScope::Hovered)
                    && subscription.component_id == target
            });
        let mut results = Vec::new();
        for subscription in subscriptions {
            if let HandlerInvocation::Matched(result) = self.invoke(&subscription, event)? {
                results.push(result);
            }
        }
        results.extend(self.deliver_pending_events()?);
        Ok(InputDispatch {
            results,
            input_pending: false,
        })
    }

    fn dispatch_global_subscriptions(
        &mut self,
        event: &EventEnvelope,
    ) -> Result<InputDispatch<A>, DispatchError> {
        let subscriptions = self
            .matching_subscriptions(event.value.as_ref().type_id(), |subscription| {
                subscription.kind == SubscriptionKind::Input(InputScope::Global)
            });
        self.invoke_input(subscriptions, event)
    }

    fn invoke_input(
        &mut self,
        subscriptions: Vec<Subscription<A>>,
        event: &EventEnvelope,
    ) -> Result<InputDispatch<A>, DispatchError> {
        let mut results = Vec::new();
        let mut input_pending = false;
        for subscription in subscriptions {
            match self.invoke(&subscription, event)? {
                HandlerInvocation::NoMatch => {}
                HandlerInvocation::AwaitingMoreInput => input_pending = true,
                HandlerInvocation::Matched(result) => results.push(result),
            }
        }
        Ok(InputDispatch {
            results,
            input_pending,
        })
    }

    fn deliver_pending_events(&mut self) -> Result<Vec<DispatchResult<A>>, DispatchError> {
        let mut results = Vec::new();
        let mut delivered_event_count = 0;
        loop {
            let event = self.pending_events.borrow_mut().pop_front();
            let Some(event) = event else {
                return Ok(results);
            };
            delivered_event_count += 1;
            if delivered_event_count > MAX_EVENTS_PER_PUBLICATION {
                self.pending_events.borrow_mut().clear();
                return Err(DispatchError::EventCycleLimitExceeded);
            }
            let subscriptions = self
                .matching_subscriptions(event.value.as_ref().type_id(), |subscription| {
                    subscription.kind == SubscriptionKind::Event
                });
            for subscription in subscriptions {
                let HandlerInvocation::Matched(result) = self.invoke(&subscription, &event)? else {
                    unreachable!("ordinary event subscriptions always match");
                };
                results.push(result);
            }
        }
    }

    fn matching_subscriptions(
        &self,
        event_type: TypeId,
        include: impl Fn(&Subscription<A>) -> bool,
    ) -> Vec<Subscription<A>> {
        self.subscriptions_by_event
            .get(&event_type)
            .into_iter()
            .flatten()
            .filter(|subscription| include(subscription))
            .cloned()
            .collect()
    }

    fn invoke(
        &mut self,
        subscription: &Subscription<A>,
        event: &EventEnvelope,
    ) -> Result<HandlerInvocation<A>, DispatchError> {
        let component = self
            .components
            .get_mut(&subscription.component_id)
            .ok_or(DispatchError::ComponentNotFound)?;
        (subscription.handler)(component.as_mut(), event.value.as_ref())
    }
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
