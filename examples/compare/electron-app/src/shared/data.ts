// Seeded data generators shared by the renderer and the main process (`--print-row0`).
// Follows examples/compare/README.md exactly (draw order, lists, formats).

export function mulberry32(seed: number): () => number {
  let a = seed | 0;
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return (t ^ (t >>> 14)) >>> 0;
  };
}

export const FIRST = [
  'Ada', 'Alan', 'Grace', 'Linus', 'Barbara', 'Ken', 'Margaret', 'Dennis',
  'Edsger', 'Frances', 'John', 'Radia', 'Donald', 'Hedy', 'Tim', 'Katherine',
] as const;
export const LAST = [
  'Lovelace', 'Turing', 'Hopper', 'Torvalds', 'Liskov', 'Thompson', 'Hamilton', 'Ritchie',
  'Dijkstra', 'Allen', 'Backus', 'Perlman', 'Knuth', 'Lamarr', 'Berners', 'Johnson',
] as const;
export const CITIES = [
  'Amsterdam', 'Berlin', 'Cairo', 'Dakar', 'Lima', 'Oslo',
  'Seoul', 'Tokyo', 'Toronto', 'Sydney', 'Nairobi', 'Madrid',
] as const;
export const COUNTRIES = [
  'Netherlands', 'Germany', 'Egypt', 'Senegal', 'Peru', 'Norway',
  'Korea', 'Japan', 'Canada', 'Australia', 'Kenya', 'Spain',
] as const;
export const DEPTS = [
  'Engineering', 'Sales', 'Marketing', 'Finance', 'Support', 'Legal', 'Design', 'Operations',
] as const;
export const STATUS = ['active', 'paused', 'pending', 'closed'] as const;

export interface Row {
  id: number;
  name: string;
  email: string;
  city: string;
  country: string;
  department: string;
  status: string;
  amount: number;
  quantity: number;
  score: number;
  active: boolean;
  created: string;
}

export const ROW_COUNT = 100_000;
const DAY_MS = 86_400_000;
const EPOCH_2022 = Date.UTC(2022, 0, 1);

export function generateRows(count: number = ROW_COUNT): Row[] {
  const r = mulberry32(42);
  const rows: Row[] = new Array(count);
  for (let i = 0; i < count; i++) {
    const first = FIRST[r() % 16];
    const last = LAST[r() % 16];
    const c = r() % 12;
    const department = DEPTS[r() % 8];
    const status = STATUS[r() % 4];
    const amount = (r() % 1_000_000) / 100;
    const quantity = r() % 500;
    const score = (r() % 10_000) / 100;
    const active = r() % 2 === 0;
    const created = new Date(EPOCH_2022 + (r() % 1500) * DAY_MS).toISOString().slice(0, 10);
    rows[i] = {
      id: i + 1,
      name: `${first} ${last}`,
      email: `${first}.${last}${i % 100}@example.com`.toLowerCase(),
      city: CITIES[c],
      country: COUNTRIES[c],
      department,
      status,
      amount,
      quantity,
      score,
      active,
      created,
    };
  }
  return rows;
}

// Dashboard feed (seed 1234).
export interface FeedSnapshot {
  users: number;
  revenue: number;
  latency: number;
  errors: number;
  throughput: { t: number; v: number }[];
  regions: { name: string; v: number }[];
}

export const REGION_NAMES = Array.from({ length: 12 }, (_, i) => `R${i + 1}`);

export function createFeed(): () => FeedSnapshot {
  const r = mulberry32(1234);
  let revenue = 0;
  let tick = 0;
  let throughput: { t: number; v: number }[] = [];
  return () => {
    const users = 1000 + (r() % 500);
    revenue += (r() % 1000) / 10;
    const latency = 20 + (r() % 80);
    const errors = (r() % 500) / 100;
    throughput = throughput.length >= 60 ? throughput.slice(1) : throughput.slice();
    throughput.push({ t: tick++, v: r() % 1000 });
    const regions = REGION_NAMES.map((name) => ({ name, v: r() % 100 }));
    return { users, revenue, latency, errors, throughput, regions };
  };
}
