//! Native structural retention scaffold, not canonical source certification.
use super::registry::{Budget, Error, Registry, Scope};
impl crate::Game {
    pub fn spatial_registry(&self) -> &Registry {
        &self.spatial_registry
    }
    pub fn register_spatial_scope(&mut self, scope: Scope, budget: &Budget) -> Result<(), Error> {
        scope.validate(budget)?;
        Ok(()) // First red: registration/retention deliberately absent.
    }
}
#[cfg(test)]
mod tests;
