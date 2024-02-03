use std::sync::Arc;

use executor::sync::AsyncRwLock;

pub struct SkipList {
    first_node: Arc<Node<T>>,
}

struct Node<T> {
    value: T,
    next: AsyncRwLock<Vec<Arc<Node<T>>>>,
}

impl<T: Default> SkipList<T> {
    async fn find_prev_chain(&self, value: &T) -> Vec<Arc<Node<T>>> {
        let mut chain = vec![];

        // Current level at which we are searching.
        // let mut level = {
        //     //  self.first_node.next.read().await.len() - 1;
        // };

        let mut node = self.first_node.clone();
        let mut level = 0; // Initialized on the first iteration.
        loop {
            let node_next = node.next.read().await;

            // Initialize the level to be the highest level in the first node in the list.
            if chain.is_empty() {
                if node_next.is_empty() {
                    chain.push(node_next);
                    break;
                }

                level = node_next.len() - 1;
            }

            // let mut done = false;
            // while level >= node_next.len() {
            //     chain.push(node.clone());

            //     if level == 0 {
            //         done = true;
            //         break;
            //     }
            //     level -= 1;
            // }

            // if done {
            //     break;
            // }

            if level < node_next.len() {
                let n = &node_next[level];
                if &n.value < value {
                    node = n.clone();
                    continue;
                }
            }

            chain.push(node);

            if level == 0 {
                break;
            }

            level -= 1;
        }

        chain
    }

    pub async fn insert(&self, value: T) {
        let mut chain = self.find_prev_chain(&value).await;

        // Update all of the pointers.
    }
}

// Find the list of all next pointers.

/*
const MIN_WIDTH_TO_SPLIT: usize = 4;

// TODO: Switch T to be something that can be cloned so that we don't have to
// create the Arc's ourselves around datatypes like 'Bytes'.
pub struct SkipList<T> {
    root: Arc<Node<T>>,
}

impl<T: Default> SkipList<T> {
    pub fn new() -> Self {
        // Root node will always start out with exactly one child.

        let value = Arc::new(T::default());

        let root_child = Node::new(value.clone(), None, None, 0);

        Self {
            root: Arc::new(Node::new(value, Some(Arc::new(root_child)), None, 0)),
        }
    }

    async fn find_prev_chain(&self, value: &T) -> Vec<Arc<Node<T>>> {
        let mut chain = vec![self.root.clone()];

        loop {
            let node = chain.last().unwrap();

            let node_state_r = node.state.upgradable_read().await;

            if let Some(next_node) = node_state_r.next.as_ref() {
                if next_node.value.as_ref() < &value {
                    let next_node = next_node.clone();
                    chain.pop();
                    chain.push(next_node);
                    continue;
                }
            }

            if let Some(child_node) = node_state_r.child.clone() {
                chain.push(child_node);
                continue;
            }

            break;
        }

        chain
    }

    // How to iterate:
    // - Iterator will store the full chain (just in case we want to skip around)
    // - While iterating, you will never see the same value twice right?
    // - If everything after the current node was deleted, then
    // - A full scan is not guranteed to be from a consistent snapshot of the system
    //   (need higher level coordination or MVCC to do that).

    // - Can you insert/remove while iterating?
    //     - Yes!

    async fn maybe_split_node(
        node: &Node<T>,
        node_state: &mut NodeState<T>,
        is_root_node: bool,
    ) -> bool {
        if node_state.next_hops < 4 {
            return false;
        }

        let first_width = node_state.next_hops / 2;
        let second_width = node_state.next_hops - first_width;

        let mut mid_node = node_state.child.clone().unwrap();
        for _ in 0..first_width {
            mid_node = mid_node.state.read().await.next.clone().unwrap()
        }

        if is_root_node {
            // For the root node, we will never set 'next'. Instead, we will create a new
            // child and defer to it

            node_state.next_hops = 0;

            let old_child = node_state.child.take();

            let child_next_node = Some(Arc::new(Node::new(
                mid_node.value.clone(),
                mid_node,
                None,
                second_width,
            )));

            node_state.child = Some(Arc::new(Node::new(
                node.value.clone(),
                old_child,
                child_next_node,
                first_width,
            )));
        } else {
            node_state.next_hops = first_width;

            let old_next = node_state.next.take();

            node_state.next = Some(Arc::new(Node::new(
                mid_node.value.clone(),
                mid_node,
                old_next,
                second_width,
            )));
        }

        true
    }

    pub async fn insert(&self, value: T) {
        let mut prev_chain = self.find_prev_chain(&value).await;

        {
            let prev_node = prev_chain.pop().unwrap();
            let mut prev_node_state = prev_node.state.write().await;

            let old_next = prev_node_state.next.take();
            prev_node_state.next = Some(Arc::new(Node::new(Arc::new(value), None, old_next, 0)));
        }

        while let Some(node) = prev_chain.pop() {
            let mut node_state = node.state.write().await;
            node_state.next_hops += 1;

            if !Self::maybe_split_node(&node, &mut node_state, prev_chain.is_empty()).await {
                break;
            }
        }

        // Step 1: Find first value <= the target value.
        // But

        // let root_skip_r = root_skip.state.upgradable_read().await;
        // match root_skip_r.
    }

    pub async fn remove(&self, value: T) {
        // Step 1 is to find the node

        let mut prev_chain = self.find_prev_chain(&value).await;

        let mut first = true;
        let mut hop_delta: isize = 0;
        while let Some(node) = prev_chain.pop() {
            let node_state_r = node.state.upgradable_read().await;

            let mut bypass_next = false;
            if let Some(next_node) = node_state_r.next.as_ref() {
                if next_node.value == value {
                    bypass_next = true;
                }
            }

            // TODO: It's also possible that we have a request to change the count from the
            // child.
            if !bypass_next && hop_delta == 0 {
                break;
            }

            let mut node_state = node_state_r.upgrade().await;

            let mut parent_hop_delta = 0;

            // let mut inherited_width = 0;
            if bypass_next {
                let next_node = node_state.next.take().unwrap();
                let next_node_state = next_node.state.read().await;

                node_state.next = next_node_state.next.clone();
                // inherited_width = next_node_state.next_hops;
                hop_delta += next_node_state.next_hops as isize;
                parent_hop_delta -= 1;
            }

            // When first = true, node_state.next_hops is 0 and should stay that way.
            if !first {
                // TODO: Assert no overflow.
                node_state.next_hops = ((node_state.next_hops as isize) + hop_delta) as usize;
            }

            // TODO: We may now have too many hops in the current layer and
            // require a balancing.
            if Self::maybe_split_node(&node, &mut node_state, prev_chain.is_empty()).await {
                parent_hop_delta += 1;
            }

            // TODO: If we do balance things, then we need to apply that to the parent.

            // TODO: If node_state.next is empty, then we should collapse this
            // layer into the next parent up.

            hop_delta = parent_hop_delta;
        }
    }
}


struct Node<T> {
    /// A special value of None is reserved for the very first element in the
    /// list which is strictly 'less than' all other possible values of T.
    value: Arc<T>,
    state: AsyncRwLock<NodeState<T>>,
}

impl<T> Node<T> {
    fn new(
        value: Arc<T>,
        child: Option<Arc<Node<T>>>,
        next: Option<Arc<Node<T>>>,
        next_hops: usize,
    ) -> Self {
        Self {
            value,
            state: AsyncRwLock::new(NodeState {
                next,
                next_hops,
                child,
            }),
        }
    }
}

struct NodeState<T> {
    next: Option<Arc<Node<T>>>,

    /// The number of nodes in the child list which are between the current node
    /// and the 'next' node.
    next_hops: usize,

    child: Option<Arc<Node<T>>>,
}
*/
