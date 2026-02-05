import { render, screen } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import { describe, it, expect } from 'vitest';
import App from './App';

describe('App', () => {
  it('renders the header with Blah³ title', () => {
    render(<App />);
    expect(screen.getByText('Blah³')).toBeInTheDocument();
  });

  it('renders all navigation tabs', () => {
    render(<App />);
    expect(screen.getByText('Dictation')).toBeInTheDocument();
    expect(screen.getByText('Reader')).toBeInTheDocument();
    expect(screen.getByText('Models')).toBeInTheDocument();
    expect(screen.getByText('Settings')).toBeInTheDocument();
  });

  it('shows Dictation panel by default', () => {
    render(<App />);
    // The Dictation tab should be active (has sky-400 color class)
    const dictationTab = screen.getByText('Dictation').closest('button');
    expect(dictationTab).toHaveClass('text-sky-400');
  });

  it('switches tabs when clicked', async () => {
    const user = userEvent.setup();
    render(<App />);

    // Click on Settings tab
    await user.click(screen.getByText('Settings'));

    // Settings tab should now be active
    const settingsTab = screen.getByText('Settings').closest('button');
    expect(settingsTab).toHaveClass('text-sky-400');

    // Dictation tab should no longer be active
    const dictationTab = screen.getByText('Dictation').closest('button');
    expect(dictationTab).not.toHaveClass('text-sky-400');
  });
});
