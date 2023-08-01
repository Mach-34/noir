use std::collections::{HashMap, HashSet};

use crate::errors::InternalError;

use super::{
    basic_block::{BasicBlock, BasicBlockId},
    function::Function,
};

/// A container for the successors and predecessors of some Block.
#[derive(Clone, Default)]
struct CfgNode {
    /// Set of blocks that containing jumps that target this block.
    /// The predecessor set has no meaningful order.
    pub(crate) predecessors: HashSet<BasicBlockId>,

    /// Set of blocks that are the targets of jumps in this block.
    /// The successors set has no meaningful order.
    pub(crate) successors: HashSet<BasicBlockId>,
}

/// The Control Flow Graph maintains a mapping of blocks to their predecessors
/// and successors where predecessors are basic blocks and successors are
/// basic blocks.
pub(crate) struct ControlFlowGraph {
    data: HashMap<BasicBlockId, CfgNode>,
}

impl ControlFlowGraph {
    /// Allocate and compute the control flow graph for `func`.
    pub(crate) fn with_function(func: &Function) -> Result<Self, InternalError> {
        // It is expected to be safe to query the control flow graph for any reachable block,
        // therefore we must ensure that a node exists for the entry block, regardless of whether
        // it later comes to describe any edges after calling compute.
        let entry_block = func.entry_block();
        let empty_node = CfgNode { predecessors: HashSet::new(), successors: HashSet::new() };
        let data = HashMap::from([(entry_block, empty_node)]);

        let mut cfg = ControlFlowGraph { data };
        cfg.compute(func)?;
        Ok(cfg)
    }

    /// Compute all of the edges between each reachable block in the function
    fn compute(&mut self, func: &Function) -> Result<(), InternalError> {
        for basic_block_id in func.reachable_blocks() {
            let basic_block = &func.dfg[basic_block_id];
            self.compute_block(basic_block_id, basic_block)?;
        }
        Ok(())
    }

    /// Compute all of the edges for the current block given
    fn compute_block(
        &mut self,
        basic_block_id: BasicBlockId,
        basic_block: &BasicBlock,
    ) -> Result<(), InternalError> {
        for dest in basic_block.successors() {
            self.add_edge(basic_block_id, dest)?;
        }
        Ok(())
    }

    /// Clears out a given block's successors. This also removes the given block from
    /// being a predecessor of any of its previous successors.
    fn invalidate_block_successors(
        &mut self,
        basic_block_id: BasicBlockId,
    ) -> Result<(), InternalError> {
        let node = match self.data.get_mut(&basic_block_id) {
            Some(node) => node,
            None => {
                return Err(InternalError::NonExistingNode {
                    extra_info: "Attempted to invalidate cfg node successors for non-existent node"
                        .to_string(),
                    location: None,
                })
            }
        };

        let old_successors = std::mem::take(&mut node.successors);

        for successor_id in old_successors {
            match self.data.get_mut(&successor_id) {
                Some(node) => {
                    node.predecessors.remove(&basic_block_id);
                }
                None => {
                    return Err(InternalError::NonExistingNode {
                        extra_info: "Cfg node successor doesn't exist".to_string(),
                        location: None,
                    })
                }
            }
        }
        Ok(())
    }

    /// Recompute the control flow graph of `block`.
    ///
    /// This is for use after modifying instructions within a specific block. It recomputes all edges
    /// from `basic_block_id` while leaving edges to `basic_block_id` intact.
    pub(crate) fn recompute_block(
        &mut self,
        func: &Function,
        basic_block_id: BasicBlockId,
    ) -> Result<(), InternalError> {
        self.invalidate_block_successors(basic_block_id)?;
        let basic_block = &func.dfg[basic_block_id];
        self.compute_block(basic_block_id, basic_block)?;
        Ok(())
    }

    /// Add a directed edge making `from` a predecessor of `to`.
    fn add_edge(&mut self, from: BasicBlockId, to: BasicBlockId) -> Result<(), InternalError> {
        let predecessor_node = self.data.entry(from).or_default();
        if predecessor_node.successors.len() >= 2 {
            return Err(InternalError::TooManyNodes {
                node_type: "successors".to_string(),
                location: None,
            });
        }
        predecessor_node.successors.insert(to);
        let successor_node = self.data.entry(to).or_default();
        if successor_node.predecessors.len() >= 2 {
            return Err(InternalError::TooManyNodes {
                node_type: "predecessors".to_string(),
                location: None,
            });
        }
        successor_node.predecessors.insert(from);
        Ok(())
    }

    /// Get an iterator over the CFG predecessors to `basic_block_id`.
    pub(crate) fn predecessors(
        &self,
        basic_block_id: BasicBlockId,
    ) -> Result<impl ExactSizeIterator<Item = BasicBlockId> + '_, InternalError> {
        match self.data.get(&basic_block_id) {
            Some(node) => Ok(node.predecessors.iter().copied()),
            None => Err(InternalError::BlockNotFound {
                node_type: "predecessors".to_string(),
                location: None,
            }),
        }
    }

    /// Get an iterator over the CFG successors to `basic_block_id`.
    pub(crate) fn successors(
        &self,
        basic_block_id: BasicBlockId,
    ) -> Result<impl ExactSizeIterator<Item = BasicBlockId> + '_, InternalError> {
        match self.data.get(&basic_block_id) {
            Some(node) => Ok(node.successors.iter().copied()),
            None => Err(InternalError::BlockNotFound {
                node_type: "successors ".to_string(),
                location: None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ssa_refactor::ir::{instruction::TerminatorInstruction, map::Id, types::Type};

    use super::{super::function::Function, ControlFlowGraph};

    #[test]
    fn empty() {
        let func_id = Id::test_new(0);
        let mut func = Function::new("func".into(), func_id);
        let block_id = func.entry_block();
        func.dfg[block_id].set_terminator(TerminatorInstruction::Return { return_values: vec![] });

        ControlFlowGraph::with_function(&func).unwrap();
    }

    #[test]
    fn jumps() {
        // Build function of form
        // fn func {
        //   block0(cond: u1):
        //     jmpif cond, then: block2, else: block1
        //   block1():
        //     jmpif cond, then: block1, else: block2
        //   block2():
        //     return ()
        // }
        let func_id = Id::test_new(0);
        let mut func = Function::new("func".into(), func_id);
        let block0_id = func.entry_block();
        let cond = func.dfg.add_block_parameter(block0_id, Type::unsigned(1));
        let block1_id = func.dfg.make_block();
        let block2_id = func.dfg.make_block();

        func.dfg[block0_id].set_terminator(TerminatorInstruction::JmpIf {
            condition: cond,
            then_destination: block2_id,
            else_destination: block1_id,
        });
        func.dfg[block1_id].set_terminator(TerminatorInstruction::JmpIf {
            condition: cond,
            then_destination: block1_id,
            else_destination: block2_id,
        });
        func.dfg[block2_id].set_terminator(TerminatorInstruction::Return { return_values: vec![] });

        let mut cfg = ControlFlowGraph::with_function(&func).unwrap();

        #[allow(clippy::needless_collect)]
        {
            let block0_predecessors: Vec<_> = cfg.predecessors(block0_id).unwrap().collect();
            let block1_predecessors: Vec<_> = cfg.predecessors(block1_id).unwrap().collect();
            let block2_predecessors: Vec<_> = cfg.predecessors(block2_id).unwrap().collect();

            let block0_successors: Vec<_> = cfg.successors(block0_id).unwrap().collect();
            let block1_successors: Vec<_> = cfg.successors(block1_id).unwrap().collect();
            let block2_successors: Vec<_> = cfg.successors(block2_id).unwrap().collect();

            assert_eq!(block0_predecessors.len(), 0);
            assert_eq!(block1_predecessors.len(), 2);
            assert_eq!(block2_predecessors.len(), 2);

            assert!(block1_predecessors.contains(&block0_id));
            assert!(block1_predecessors.contains(&block1_id));
            assert!(block2_predecessors.contains(&block0_id));
            assert!(block2_predecessors.contains(&block1_id));

            assert_eq!(block0_successors.len(), 2);
            assert_eq!(block1_successors.len(), 2);
            assert_eq!(block2_successors.len(), 0);

            assert!(block0_successors.contains(&block1_id));
            assert!(block0_successors.contains(&block2_id));
            assert!(block1_successors.contains(&block1_id));
            assert!(block1_successors.contains(&block2_id));
        }

        // Modify function to form:
        // fn func {
        //   block0(cond: u1):
        //     jmpif cond, then: block1, else: ret_block
        //   block1():
        //     jmpif cond, then: block1, else: block2
        //   block2():
        //     jmp ret_block()
        //   ret_block():
        //     return ()
        // }
        let ret_block_id = func.dfg.make_block();
        func.dfg[ret_block_id]
            .set_terminator(TerminatorInstruction::Return { return_values: vec![] });
        func.dfg[block2_id].set_terminator(TerminatorInstruction::Jmp {
            destination: ret_block_id,
            arguments: vec![],
        });
        func.dfg[block0_id].set_terminator(TerminatorInstruction::JmpIf {
            condition: cond,
            then_destination: block1_id,
            else_destination: ret_block_id,
        });

        // Recompute new and changed blocks
        cfg.recompute_block(&func, block0_id).unwrap();
        cfg.recompute_block(&func, block2_id).unwrap();
        cfg.recompute_block(&func, ret_block_id).unwrap();

        #[allow(clippy::needless_collect)]
        {
            let block0_predecessors: Vec<_> = cfg.predecessors(block0_id).unwrap().collect();
            let block1_predecessors: Vec<_> = cfg.predecessors(block1_id).unwrap().collect();
            let block2_predecessors: Vec<_> = cfg.predecessors(block2_id).unwrap().collect();

            let block0_successors: Vec<_> = cfg.successors(block0_id).unwrap().collect();
            let block1_successors: Vec<_> = cfg.successors(block1_id).unwrap().collect();
            let block2_successors: Vec<_> = cfg.successors(block2_id).unwrap().collect();

            assert_eq!(block0_predecessors.len(), 0);
            assert_eq!(block1_predecessors.len(), 2);
            assert_eq!(block2_predecessors.len(), 1);

            assert!(block1_predecessors.contains(&block0_id));
            assert!(block1_predecessors.contains(&block1_id));
            assert!(!block2_predecessors.contains(&block0_id));
            assert!(block2_predecessors.contains(&block1_id));

            assert_eq!(block0_successors.len(), 2);
            assert_eq!(block1_successors.len(), 2);
            assert_eq!(block2_successors.len(), 1);

            assert!(block0_successors.contains(&block1_id));
            assert!(block0_successors.contains(&ret_block_id));
            assert!(block1_successors.contains(&block1_id));
            assert!(block1_successors.contains(&block2_id));
            assert!(block2_successors.contains(&ret_block_id));
        }
    }
}
