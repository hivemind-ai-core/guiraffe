//! Seeded data shared with the Electron app (contract: `bench/electron/APPS.md`).
//! Pure logic, no GPUI.

/// `mulberry32`, bit-for-bit the JavaScript version (returns the unsigned 32-bit draw).
#[derive(Clone)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Self(seed)
    }
    pub fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_add(0x6d2b_79f5);
        let a = self.0;
        let mut t = (a ^ (a >> 15)).wrapping_mul(1 | a);
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
        t ^ (t >> 14)
    }
    fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[self.next_u32() as usize % xs.len()]
    }
}

pub const FIRST: [&str; 16] = [
    "Ada",
    "Alan",
    "Grace",
    "Linus",
    "Barbara",
    "Ken",
    "Margaret",
    "Dennis",
    "Edsger",
    "Frances",
    "John",
    "Radia",
    "Donald",
    "Hedy",
    "Tim",
    "Katherine",
];
pub const LAST: [&str; 16] = [
    "Lovelace", "Turing", "Hopper", "Torvalds", "Liskov", "Thompson", "Hamilton", "Ritchie",
    "Dijkstra", "Allen", "Backus", "Perlman", "Knuth", "Lamarr", "Berners", "Johnson",
];
pub const CITIES: [&str; 12] = [
    "Amsterdam",
    "Berlin",
    "Cairo",
    "Dakar",
    "Lima",
    "Oslo",
    "Seoul",
    "Tokyo",
    "Toronto",
    "Sydney",
    "Nairobi",
    "Madrid",
];
pub const COUNTRIES: [&str; 12] = [
    "Netherlands",
    "Germany",
    "Egypt",
    "Senegal",
    "Peru",
    "Norway",
    "Korea",
    "Japan",
    "Canada",
    "Australia",
    "Kenya",
    "Spain",
];
pub const DEPTS: [&str; 8] = [
    "Engineering",
    "Sales",
    "Marketing",
    "Finance",
    "Support",
    "Legal",
    "Design",
    "Operations",
];
pub const STATUS: [&str; 4] = ["active", "paused", "pending", "closed"];

pub const ROWS: usize = 100_000;

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub id: i64,
    pub name: String,
    pub email: String,
    pub city: &'static str,
    pub country: &'static str,
    pub department: &'static str,
    pub status: &'static str,
    pub amount: f64,
    pub quantity: i64,
    pub score: f64,
    pub active: bool,
    pub created: String,
}

/// `YYYY-MM-DD` of 2022-01-01 plus `days` (proleptic Gregorian, Howard Hinnant's algorithm).
pub fn date_from_days(days: u32) -> String {
    // Days from 1970-01-01 to 2022-01-01.
    let z = 18_993 + i64::from(days) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn generate(n: usize) -> Vec<Row> {
    let mut r = Rng::new(42);
    (0..n)
        .map(|i| {
            let first = r.pick(&FIRST);
            let last = r.pick(&LAST);
            let c = r.next_u32() as usize % CITIES.len();
            let department = r.pick(&DEPTS);
            let status = r.pick(&STATUS);
            let amount = f64::from(r.next_u32() % 1_000_000) / 100.;
            let quantity = i64::from(r.next_u32() % 500);
            let score = f64::from(r.next_u32() % 10_000) / 100.;
            let active = r.next_u32().is_multiple_of(2);
            let created = date_from_days(r.next_u32() % 1500);
            Row {
                id: i as i64 + 1,
                name: format!("{first} {last}"),
                email: format!("{first}.{last}{}@example.com", i % 100).to_lowercase(),
                city: CITIES[c],
                country: COUNTRIES[c],
                department,
                status,
                amount,
                quantity,
                score,
                active,
                created,
            }
        })
        .collect()
}

pub fn row_json(r: &Row) -> serde_json::Value {
    serde_json::json!({
        "id": r.id, "name": r.name, "email": r.email, "city": r.city, "country": r.country,
        "department": r.department, "status": r.status, "amount": r.amount,
        "quantity": r.quantity, "score": r.score, "active": r.active, "created": r.created,
    })
}

/// Case-insensitive (ASCII) substring test; `needle` must already be lowercase.
pub fn contains_ci(hay: &str, needle: &str) -> bool {
    needle.is_empty()
        || hay
            .as_bytes()
            .windows(needle.len())
            .any(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
}

// ------------------------------------------------------------------ dashboard feed

pub const POINTS: usize = 60;
pub const REGIONS: usize = 12;

#[derive(Clone)]
pub struct Feed {
    rng: Rng,
    pub users: u32,
    pub revenue: f64,
    pub latency: u32,
    pub errors: f64,
    pub throughput: std::collections::VecDeque<u32>,
    pub regions: [u32; REGIONS],
}

impl Default for Feed {
    fn default() -> Self {
        Self::new()
    }
}

impl Feed {
    /// The feed after its 60 warm-up ticks.
    pub fn new() -> Self {
        let mut f = Self {
            rng: Rng::new(1234),
            users: 0,
            revenue: 0.,
            latency: 0,
            errors: 0.,
            throughput: std::collections::VecDeque::with_capacity(POINTS + 1),
            regions: [0; REGIONS],
        };
        for _ in 0..POINTS {
            f.tick();
        }
        f
    }

    pub fn tick(&mut self) {
        let r = &mut self.rng;
        self.users = 1000 + r.next_u32() % 500;
        self.revenue += f64::from(r.next_u32() % 1000) / 10.;
        self.latency = 20 + r.next_u32() % 80;
        self.errors = f64::from(r.next_u32() % 500) / 100.;
        self.throughput.push_back(r.next_u32() % 1000);
        while self.throughput.len() > POINTS {
            self.throughput.pop_front();
        }
        for v in &mut self.regions {
            *v = r.next_u32() % 100;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mulberry32_matches_javascript() {
        // node -e 'mulberry32(42)' first three draws.
        let mut r = Rng::new(42);
        let a = r.next_u32();
        let b = r.next_u32();
        assert_ne!(a, b);
        // Row 0 as printed by the Electron app's `--print-row0`.
        let row = &generate(1)[0];
        assert_eq!(row.name, "Donald Backus");
        assert_eq!(row.email, "donald.backus0@example.com");
        assert_eq!(row.city, "Amsterdam");
        assert_eq!(row.country, "Netherlands");
        assert_eq!(row.department, "Legal");
        assert_eq!(row.status, "pending");
        assert_eq!(row.amount, 6977.47);
        assert_eq!(row.quantity, 300);
        assert_eq!(row.score, 78.57);
        assert!(row.active);
        assert_eq!(row.created, "2025-07-29");
    }

    #[test]
    fn dates() {
        assert_eq!(date_from_days(0), "2022-01-01");
        assert_eq!(date_from_days(31), "2022-02-01");
        assert_eq!(date_from_days(365), "2023-01-01");
        assert_eq!(date_from_days(365 + 365 + 31 + 28), "2024-02-29");
    }

    #[test]
    fn substring() {
        assert!(contains_ci("Ada Lovelace", "ada"));
        assert!(contains_ci("Hedy Lamarr", "lam"));
        assert!(!contains_ci("Ken Thompson", "ada"));
        assert!(contains_ci("x", ""));
    }

    #[test]
    fn feed_window() {
        let mut f = Feed::new();
        assert_eq!(f.throughput.len(), POINTS);
        f.tick();
        assert_eq!(f.throughput.len(), POINTS);
        assert!((1000..1500).contains(&f.users));
    }
}
