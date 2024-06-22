use axum::Router;
use crate::Link;

pub trait Feature {
    fn name(&self) -> String;

    fn link(&self) -> Option<Link> {
        return None;
    }

    fn api(&self) -> Option<Router> {
        return None;
    }

    fn web(&self) -> Option<Router> {
        return None;
    }

    // Cannot use these methods and remain Object Safe
    // fn template(_template: impl Template);
    // fn state<T: Sized>(&self) -> Option<Box<(dyn FeatureState<State = T> + 'static )>>;
}


pub type FeatureError = Box<dyn std::error::Error>;