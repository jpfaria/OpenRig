    use super::*;

    #[test]
    fn new_buffer_starts_empty() {
        let buf = LookaheadBuffer::new(8);
        assert_eq!(buf.len(), 8);
        assert_eq!(buf.peak(), 0.0);
    }

    #[test]
    fn push_delays_by_len_samples() {
        let mut buf = LookaheadBuffer::new(4);
        // First 4 pushes return the initial zeros
        for _ in 0..4 {
            assert_eq!(buf.push(1.0), 0.0);
        }
        // Subsequent pushes return the 1.0 we pushed earlier
        assert_eq!(buf.push(2.0), 1.0);
        assert_eq!(buf.push(2.0), 1.0);
    }

    #[test]
    fn peak_reports_max_abs_over_window() {
        let mut buf = LookaheadBuffer::new(3);
        buf.push(0.2);
        assert_eq!(buf.peak(), 0.2);
        buf.push(-0.8);
        assert_eq!(buf.peak(), 0.8);
        buf.push(0.5);
        assert_eq!(buf.peak(), 0.8);
        // 0.2 falls out of the window; 0.8 still in
        buf.push(0.1);
        assert_eq!(buf.peak(), 0.8);
        // 0.8 falls out; remaining are 0.5, 0.1, and new value
        buf.push(0.3);
        assert_eq!(buf.peak(), 0.5);
    }

    #[test]
    fn clear_resets_state() {
        let mut buf = LookaheadBuffer::new(4);
        buf.push(1.0);
        buf.push(0.5);
        buf.clear();
        assert_eq!(buf.peak(), 0.0);
        assert_eq!(buf.push(0.1), 0.0);
    }

    #[test]
    fn zero_len_is_coerced_to_one() {
        let mut buf = LookaheadBuffer::new(0);
        assert_eq!(buf.len(), 1);
        assert_eq!(buf.push(0.7), 0.0);
        assert_eq!(buf.push(0.2), 0.7);
    }
