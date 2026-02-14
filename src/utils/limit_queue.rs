// Fixed-size queue that removes oldest items when limit is exceeded
use std::collections::VecDeque;

/// A queue that automatically removes the oldest element when the limit is exceeded
///
/// This is useful for maintaining a fixed-size buffer, such as terminal output history.
#[derive(Debug, Clone)]
pub struct LimitQueue<T> {
    queue: VecDeque<T>,
    limit: usize,
    on_exceed: Option<fn(&T)>,
}

impl<T> LimitQueue<T> {
    /// Create a new LimitQueue with the specified limit
    ///
    /// # Arguments
    /// * `limit` - Maximum number of items in the queue
    pub fn new(limit: usize) -> Self {
        Self {
            queue: VecDeque::with_capacity(limit),
            limit,
            on_exceed: None,
        }
    }

    /// Set a callback to be called when an item is removed due to exceeding the limit
    ///
    /// # Arguments
    /// * `callback` - Function to call with the removed item
    #[allow(dead_code)]
    pub fn on_exceed(mut self, callback: fn(&T)) -> Self {
        self.on_exceed = Some(callback);
        self
    }

    /// Push an item to the queue
    ///
    /// If the queue is at the limit, the oldest item will be removed first.
    ///
    /// # Arguments
    /// * `item` - The item to push
    pub fn push(&mut self, item: T) {
        self.queue.push_back(item);

        if self.queue.len() > self.limit {
            if let Some(removed) = self.queue.pop_front() {
                if let Some(callback) = self.on_exceed {
                    callback(&removed);
                }
            }
        }
    }

    /// Get the number of items in the queue
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Check if the queue is empty
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Get an iterator over the items in the queue
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.queue.iter()
    }

    /// Get a mutable iterator over the items in the queue
    #[allow(dead_code)]
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.queue.iter_mut()
    }

    /// Get a reference to an item at the specified index
    #[allow(dead_code)]
    pub fn get(&self, index: usize) -> Option<&T> {
        self.queue.get(index)
    }

    /// Clear all items from the queue
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.queue.clear();
    }

    /// Get the limit of the queue
    #[allow(dead_code)]
    pub fn limit(&self) -> usize {
        self.limit
    }
}

impl<T> Default for LimitQueue<T> {
    fn default() -> Self {
        Self::new(100)
    }
}

// Implement Index trait for convenient access
impl<T> std::ops::Index<usize> for LimitQueue<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.queue[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_limit_queue_basic() {
        let mut queue = LimitQueue::new(3);

        queue.push(1);
        queue.push(2);
        queue.push(3);

        assert_eq!(queue.len(), 3);
        assert_eq!(queue.get(0), Some(&1));
        assert_eq!(queue.get(1), Some(&2));
        assert_eq!(queue.get(2), Some(&3));
    }

    #[test]
    fn test_limit_queue_exceeds_limit() {
        let mut queue = LimitQueue::new(3);

        queue.push(1);
        queue.push(2);
        queue.push(3);
        queue.push(4); // This should remove the first item (1)

        assert_eq!(queue.len(), 3);
        assert_eq!(queue.get(0), Some(&2));
        assert_eq!(queue.get(1), Some(&3));
        assert_eq!(queue.get(2), Some(&4));
    }

    #[test]
    fn test_limit_queue_continues_removing() {
        let mut queue = LimitQueue::new(2);

        queue.push(1);
        queue.push(2);
        queue.push(3);
        queue.push(4);
        queue.push(5);

        assert_eq!(queue.len(), 2);
        assert_eq!(queue.get(0), Some(&4));
        assert_eq!(queue.get(1), Some(&5));
    }

    #[test]
    fn test_limit_queue_iter() {
        let mut queue = LimitQueue::new(3);

        queue.push(1);
        queue.push(2);
        queue.push(3);

        let items: Vec<_> = queue.iter().copied().collect();
        assert_eq!(items, vec![1, 2, 3]);
    }

    #[test]
    fn test_limit_queue_clear() {
        let mut queue = LimitQueue::new(3);

        queue.push(1);
        queue.push(2);

        assert_eq!(queue.len(), 2);

        queue.clear();

        assert_eq!(queue.len(), 0);
        assert!(queue.is_empty());
    }

    #[test]
    fn test_limit_queue_index() {
        let mut queue = LimitQueue::new(3);

        queue.push(10);
        queue.push(20);
        queue.push(30);

        assert_eq!(queue[0], 10);
        assert_eq!(queue[1], 20);
        assert_eq!(queue[2], 30);
    }
}
