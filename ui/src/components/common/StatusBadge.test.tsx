import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import StatusBadge from './StatusBadge';

// Colors per SPEC-001: Running=green, Building/Deploying/Pending=yellow, Degraded=orange, Failed=red.
describe('StatusBadge', () => {
  it.each([
    ['Pending', 'yellow'],
    ['Building', 'yellow'],
    ['Deploying', 'yellow'],
    ['Running', 'green'],
    ['Degraded', 'orange'],
    ['Failed', 'red'],
  ] as const)('renders %s with the %s color class', (status, color) => {
    render(<StatusBadge status={status} />);
    const badge = screen.getByText(status);
    expect(badge.className).toContain(color);
  });
});
