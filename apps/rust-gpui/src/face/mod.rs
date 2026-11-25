//! Native cutout renderer for the Grok Bot 1.2.3 motion recipes.
mod clips;
mod contours;
mod element;
mod engine;
mod expressions;
mod field;
mod math;
mod morphs;
mod outline_data;
mod pumpkin;
mod render;
mod shapes;

pub(crate) use element::{BotFaceElement, FaceLayer};
pub use element::BotFaceOptions;

#[cfg(test)]
mod tests;
