import { memo, useEffect, useState } from 'react';
import { Card, Group, SimpleGrid, Text } from '@mantine/core';
import {
  Bar,
  BarChart,
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  XAxis,
  YAxis,
} from 'recharts';
import { createFeed, type FeedSnapshot } from '../../shared/data';

// The feed lives for the app's lifetime; it only ticks while the dashboard is mounted.
const nextTick = createFeed();
let latest: FeedSnapshot | null = null;

function initialSnapshot(): FeedSnapshot {
  if (!latest) {
    let s = nextTick();
    for (let i = 1; i < 60; i++) s = nextTick();
    latest = s;
  }
  return latest;
}

const StatCard = memo(function StatCard({ label, value }: { label: string; value: string }) {
  return (
    <Card withBorder padding="md">
      <Text size="sm" c="dimmed">
        {label}
      </Text>
      <Text size="xl" fw={700}>
        {value}
      </Text>
    </Card>
  );
});

const ThroughputChart = memo(function ThroughputChart({ data }: { data: FeedSnapshot['throughput'] }) {
  return (
    <ResponsiveContainer width="100%" height="100%">
      <LineChart data={data} margin={{ top: 8, right: 16, bottom: 0, left: 0 }}>
        <CartesianGrid strokeDasharray="3 3" stroke="var(--mantine-color-default-border)" />
        <XAxis dataKey="t" tick={false} stroke="var(--mantine-color-dimmed)" />
        <YAxis domain={[0, 1000]} stroke="var(--mantine-color-dimmed)" />
        <Line
          type="monotone"
          dataKey="v"
          stroke="var(--mantine-color-indigo-6)"
          strokeWidth={2}
          dot={false}
          isAnimationActive={false}
        />
      </LineChart>
    </ResponsiveContainer>
  );
});

const RegionsChart = memo(function RegionsChart({ data }: { data: FeedSnapshot['regions'] }) {
  return (
    <ResponsiveContainer width="100%" height="100%">
      <BarChart data={data} margin={{ top: 8, right: 16, bottom: 0, left: 0 }}>
        <CartesianGrid strokeDasharray="3 3" stroke="var(--mantine-color-default-border)" />
        <XAxis dataKey="name" stroke="var(--mantine-color-dimmed)" />
        <YAxis domain={[0, 100]} stroke="var(--mantine-color-dimmed)" />
        <Bar dataKey="v" fill="var(--mantine-color-teal-6)" isAnimationActive={false} />
      </BarChart>
    </ResponsiveContainer>
  );
});

export function Dashboard() {
  const [snap, setSnap] = useState<FeedSnapshot>(initialSnapshot);

  useEffect(() => {
    const id = setInterval(() => {
      latest = nextTick();
      setSnap(latest);
    }, 1000);
    return () => clearInterval(id);
  }, []);

  return (
    <div className="page">
      <SimpleGrid cols={4}>
        <StatCard label="Active users" value={snap.users.toLocaleString('en-US')} />
        <StatCard label="Revenue" value={`$${snap.revenue.toFixed(2)}`} />
        <StatCard label="p50 latency" value={`${snap.latency} ms`} />
        <StatCard label="Error rate" value={`${snap.errors.toFixed(2)}%`} />
      </SimpleGrid>
      <Card withBorder style={{ flex: 1, minHeight: 0 }}>
        <Group justify="space-between" mb="xs">
          <Text fw={600}>Throughput</Text>
          <Text size="sm" c="dimmed">
            last 60 s
          </Text>
        </Group>
        <div style={{ flex: 1, minHeight: 0 }}>
          <ThroughputChart data={snap.throughput} />
        </div>
      </Card>
      <Card withBorder style={{ flex: 1, minHeight: 0 }}>
        <Text fw={600} mb="xs">
          Regions
        </Text>
        <div style={{ flex: 1, minHeight: 0 }}>
          <RegionsChart data={snap.regions} />
        </div>
      </Card>
    </div>
  );
}
