use std::collections::VecDeque;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Task {
    pub page: usize,
    pub width: i32,
}
#[derive(Default)]
pub struct Queue {
    tasks: VecDeque<Task>,
}
impl Queue {
    pub fn replace(&mut self, tasks: Vec<Task>) {
        self.tasks = tasks.into();
    }
    pub fn next(&mut self) -> Option<Task> {
        self.tasks.pop_front()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn viewport_replaces_obsolete_zoom_work_and_prioritizes_drafts() {
        let mut queue = Queue::default();
        queue.replace(vec![
            Task {
                page: 0,
                width: 768,
            },
            Task {
                page: 0,
                width: 1856,
            },
            Task {
                page: 1,
                width: 768,
            },
        ]);
        assert_eq!(
            queue.next(),
            Some(Task {
                page: 0,
                width: 768
            })
        );
        queue.replace(vec![
            Task {
                page: 9,
                width: 768,
            },
            Task {
                page: 9,
                width: 2400,
            },
        ]);
        assert_eq!(
            queue.next(),
            Some(Task {
                page: 9,
                width: 768
            })
        );
        assert_eq!(
            queue.next(),
            Some(Task {
                page: 9,
                width: 2400
            })
        );
        assert_eq!(queue.next(), None);
    }
}
