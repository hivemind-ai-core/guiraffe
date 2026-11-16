//! Bounded lock-free hand-off. Crossbeam owns the queue's audited synchronization;
//! this crate adds cancellation, end-of-stream and bounded token-batch sizes.
//! The producer never waits on a UI lock. `Full` returns ownership for retry/backpressure.
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use crossbeam_queue::ArrayQueue;

pub const MAX_CHUNK_BYTES:usize=64*1024;
struct Shared { queue:ArrayQueue<String>, cancelled:AtomicBool, finished:AtomicBool }
#[derive(Clone)]
pub struct Cancellation(Arc<Shared>);
impl Cancellation {
    pub fn cancel(&self) {self.0.cancelled.store(true,Ordering::Release);}
    pub fn is_cancelled(&self)->bool {self.0.cancelled.load(Ordering::Acquire)}
}
pub struct Producer(Arc<Shared>);
pub struct Consumer(Arc<Shared>);
#[derive(Debug, PartialEq, Eq)]
pub enum PushError {Full(String),Closed(String),TooLarge(String)}
/// Capacity is clamped to at least one; both endpoints are deliberately non-clonable.
pub fn channel(capacity:usize)->(Producer,Consumer) {
    let shared=Arc::new(Shared {queue:ArrayQueue::new(capacity.max(1)),cancelled:AtomicBool::new(false),finished:AtomicBool::new(false)});
    (Producer(shared.clone()),Consumer(shared))
}
impl Producer {
    pub fn push(&self,chunk:String)->Result<(),PushError> {
        if self.0.cancelled.load(Ordering::Acquire) {return Err(PushError::Closed(chunk));}
        if chunk.len()>MAX_CHUNK_BYTES {return Err(PushError::TooLarge(chunk));}
        self.0.queue.push(chunk).map_err(PushError::Full)
    }
    pub fn is_cancelled(&self)->bool {self.0.cancelled.load(Ordering::Acquire)}
}
impl Drop for Producer {fn drop(&mut self) {self.0.finished.store(true,Ordering::Release);}}
impl Consumer {
    pub fn cancellation(&self)->Cancellation {Cancellation(self.0.clone())}
    /// Drain at most `max_chunks`, bounding the work (and allocation) in one UI frame.
    pub fn drain(&self,max_chunks:usize)->String {
        let mut batch=String::new();
        if self.is_cancelled() {return batch;}
        for _ in 0..max_chunks {match self.0.queue.pop() {Some(chunk)=>batch.push_str(&chunk),None=>break}}
        batch
    }
    pub fn is_cancelled(&self)->bool {self.0.cancelled.load(Ordering::Acquire)}
    /// Acquire the completion publication before checking emptiness, including the final chunk.
    pub fn is_finished(&self)->bool {self.0.finished.load(Ordering::Acquire)&&self.0.queue.is_empty()}
}
impl Drop for Consumer {fn drop(&mut self) {self.0.cancelled.store(true,Ordering::Release);}}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn capacity_backpressure_fifo_and_completion() {let(p,c)=channel(2);assert!(p.push("a".into()).is_ok());assert!(p.push("b".into()).is_ok());assert_eq!(p.push("c".into()),Err(PushError::Full("c".into())));assert_eq!(c.drain(1),"a");assert!(p.push("c".into()).is_ok());drop(p);assert!(!c.is_finished());assert_eq!(c.drain(9),"bc");assert!(c.is_finished());}
    #[test] fn cancellation_discards_queued_content() {let(p,c)=channel(1);p.push("stale".into()).unwrap();c.cancellation().cancel();assert_eq!(c.drain(10),"");assert!(matches!(p.push("late".into()),Err(PushError::Closed(_))));}
    #[test] fn dropping_consumer_stops_producer() {let(p,c)=channel(0);drop(c);assert!(p.is_cancelled());}
    #[test] fn empty_finish_and_bounded_batches() {let(p,c)=channel(1);assert_eq!(c.drain(0),"");assert!(matches!(p.push("x".repeat(MAX_CHUNK_BYTES+1)),Err(PushError::TooLarge(_))));assert!(!c.is_finished());drop(p);assert!(c.is_finished());}
    #[test] fn concurrent_delivery_exactly_once_across_capacities() {
        for capacity in [1,2,7,64] {let(p,c)=channel(capacity);let worker=std::thread::spawn(move||{for n in 0..10_000 {let mut text=format!("{n},");loop {match p.push(text) {Ok(())=>break,Err(PushError::Full(value))=>{text=value;std::thread::yield_now();},Err(e)=>panic!("unexpected {e:?}")}}}});
            let mut result=String::new();loop {result.push_str(&c.drain(8));if c.is_finished() {break;}std::thread::yield_now();}worker.join().unwrap();let expected=(0..10_000).map(|n|format!("{n},")).collect::<String>();assert_eq!(result,expected);}
    }
}
