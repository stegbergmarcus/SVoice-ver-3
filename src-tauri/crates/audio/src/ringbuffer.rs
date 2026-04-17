use ringbuf::{traits::*, HeapRb};
use std::sync::Arc;

pub struct AudioRing {
    producer: Arc<std::sync::Mutex<ringbuf::HeapProd<f32>>>,
    consumer: Arc<std::sync::Mutex<ringbuf::HeapCons<f32>>>,
}

impl AudioRing {
    /// `capacity` i sekunder vid 16 kHz (t.ex. 30s → 480 000 samples).
    pub fn new(capacity_samples: usize) -> Self {
        let rb = HeapRb::<f32>::new(capacity_samples);
        let (producer, consumer) = rb.split();
        Self {
            producer: Arc::new(std::sync::Mutex::new(producer)),
            consumer: Arc::new(std::sync::Mutex::new(consumer)),
        }
    }

    /// Skriv samples från audio-callback-tråd. Returnerar antal faktiskt skrivna
    /// (om buffer är full skrivs färre).
    pub fn push_samples(&self, samples: &[f32]) -> usize {
        let mut p = self.producer.lock().unwrap();
        p.push_slice(samples)
    }

    /// Läs ut alla tillgängliga samples (drainar buffer).
    pub fn drain(&self) -> Vec<f32> {
        let mut c = self.consumer.lock().unwrap();
        let len = c.occupied_len();
        let mut out = vec![0.0; len];
        c.pop_slice(&mut out);
        out
    }

    /// Klär buffer utan att returnera innehållet.
    pub fn clear(&self) {
        let mut c = self.consumer.lock().unwrap();
        while c.try_pop().is_some() {}
    }

    pub fn len(&self) -> usize {
        self.consumer.lock().unwrap().occupied_len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_drain_roundtrip() {
        let ring = AudioRing::new(100);
        let written = ring.push_samples(&[0.1, 0.2, 0.3]);
        assert_eq!(written, 3);
        let drained = ring.drain();
        assert_eq!(drained, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn overflow_discards_excess() {
        let ring = AudioRing::new(3);
        let written = ring.push_samples(&[0.1, 0.2, 0.3, 0.4, 0.5]);
        assert_eq!(written, 3);
    }
}
