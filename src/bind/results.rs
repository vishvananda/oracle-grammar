//! Native result addresses, allocated in lexical instruction order. Target slots
//! and source-token positions are different address spaces and never enter here.
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(super) struct ResultId(u32);

#[derive(Debug, Default, Clone)]
pub(super) struct Results {
    // One wider than the native address so allocating u32::MAX does not wrap.
    next: u64,
    // Usually empty: ordinary cards keep the allocation-free cursor path.
    generated: Vec<ResultId>,
}

impl Results {
    pub(super) fn allocate(&mut self) -> Result<ResultId, String> {
        let id = u32::try_from(self.next).map_err(|_| "result address space exhausted")?;
        self.next += 1;
        Ok(ResultId(id))
    }

    /// Generated definitions consume native addresses, never lexical positions.
    pub(super) fn allocate_generated(&mut self) -> Result<ResultId, String> {
        let id = u32::try_from(self.next).map_err(|_| "result address space exhausted")?;
        self.next += 1;
        let id = ResultId(id);
        self.generated.push(id);
        Ok(id)
    }

    pub(super) fn lexical_issued(&self) -> u64 {
        self.next - self.generated.len() as u64
    }

    #[cfg(test)]
    pub(super) fn issued(&self) -> u64 {
        self.next
    }

    /// Preserve the grammar's historical saturating backward references. Native
    /// validation still decides whether an address has a visible definition.
    pub(super) fn back(&self, distance: u64) -> Result<ResultId, String> {
        let mut address = if distance == 0 {
            self.next
        } else {
            let mut address = self.lexical_issued().saturating_sub(distance);
            for generated in &self.generated {
                if u64::from(generated.0) > address {
                    break;
                }
                address += 1;
            }
            address
        };
        // An entirely synthetic prefix has no lexical predecessor. Retain the
        // historical zero underflow; native validation decides its visibility.
        if distance > self.lexical_issued() {
            address = 0;
        }
        u32::try_from(address)
            .map(ResultId)
            .map_err(|_| "result reference outside native address space".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_exhaustion_never_wraps_or_reuses_a_definition() {
        let mut ids = Results {
            next: u64::from(u32::MAX),
            generated: Vec::new(),
        };
        assert_eq!(ids.allocate().unwrap(), ResultId(u32::MAX));
        assert!(ids.allocate().is_err());
        assert_eq!(ids.issued(), u64::from(u32::MAX) + 1);
        assert_eq!(ids.back(1).unwrap(), ResultId(u32::MAX));
        assert!(ids.back(0).is_err());
    }

    #[test]
    fn generated_definitions_preserve_lexical_back_references() {
        let mut ids = Results::default();
        assert_eq!(ids.allocate().unwrap(), ResultId(0));
        assert_eq!(ids.allocate_generated().unwrap(), ResultId(1));
        assert_eq!(ids.allocate().unwrap(), ResultId(2));
        assert_eq!(ids.back(2).unwrap(), ResultId(0));
        assert_eq!(ids.back(1).unwrap(), ResultId(2));
        assert_eq!(ids.allocate_generated().unwrap(), ResultId(3));
        assert_eq!(ids.allocate_generated().unwrap(), ResultId(4));
        assert_eq!(ids.allocate().unwrap(), ResultId(5));
        assert_eq!(ids.back(2).unwrap(), ResultId(2));
        assert_eq!(ids.back(3).unwrap(), ResultId(0));
        assert_eq!(ids.issued(), 6);
    }

    #[test]
    fn ordinary_results_allocate_no_history_and_keep_saturation() {
        let mut ids = Results::default();
        for expected in 0..100 {
            assert_eq!(ids.allocate().unwrap(), ResultId(expected));
        }
        assert!(ids.generated.is_empty());
        assert_eq!(ids.lexical_issued(), 100);
        assert_eq!(ids.back(0).unwrap(), ResultId(100));
        assert_eq!(ids.back(101).unwrap(), ResultId(0));
    }

    #[test]
    fn generated_prefix_and_tail_preserve_native_and_lexical_cursors() {
        let mut ids = Results::default();
        assert_eq!(ids.allocate_generated().unwrap(), ResultId(0));
        assert_eq!(ids.allocate().unwrap(), ResultId(1));
        assert_eq!(ids.allocate_generated().unwrap(), ResultId(2));
        assert_eq!(ids.back(0).unwrap(), ResultId(3));
        assert_eq!(ids.back(1).unwrap(), ResultId(1));
        assert_eq!(ids.back(2).unwrap(), ResultId(0));
        assert_eq!(ids.lexical_issued(), 1);
    }

    #[test]
    fn independent_scopes_and_explicit_shared_cursors_keep_reading_order() {
        let mut outer = Results::default();
        assert_eq!(outer.allocate().unwrap(), ResultId(0));
        let mut fresh = Results::default();
        assert_eq!(fresh.allocate().unwrap(), ResultId(0));
        let mut reflexive = outer.clone();
        assert_eq!(reflexive.allocate().unwrap(), ResultId(1));
        outer = reflexive;
        assert_eq!(outer.allocate().unwrap(), ResultId(2));
        assert_eq!(outer.back(2).unwrap(), ResultId(1));
        assert_eq!(fresh.back(20).unwrap(), ResultId(0));
    }
}
