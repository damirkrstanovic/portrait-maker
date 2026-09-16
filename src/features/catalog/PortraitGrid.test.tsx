import { fireEvent, render, screen } from "@testing-library/react";
import { vi } from "vitest";

import type { Portrait } from "../../lib/contracts";
import { PortraitGrid } from "./PortraitGrid";

const portraits: Portrait[] = Array.from({ length: 10_000 }, (_, index) => ({
  id: `portrait-${index}`,
  sourceId: `source-${index % 4}`,
  name: `Portrait ${index}`,
  sourceName: `Source ${index % 4}`,
  originalFolder: `portrait-${index}`,
  description: null,
  labels: [],
  selected: index === 3,
  trashedAt: null,
}));

test("keeps rendered portrait cards below 200 while scrolling 10,000 matching portraits", async () => {
  render(
    <PortraitGrid
      items={portraits}
      total={portraits.length}
      role="large"
      assetUrl={(id) => `portrait://thumbnail/${id}/large/360`}
      onFocus={vi.fn()}
      onToggleSelection={vi.fn()}
    />,
  );
  const viewport = screen.getByRole("region", { name: "Portrait results" });
  Object.defineProperty(viewport, "clientHeight", { configurable: true, value: 720 });
  Object.defineProperty(viewport, "clientWidth", { configurable: true, value: 1200 });
  fireEvent.scroll(viewport, { target: { scrollTop: 48_000 } });

  expect((await screen.findAllByTestId("portrait-card")).length).toBeLessThan(200);
});

test("hides retained rows during a replacement request and provides loading, error, empty, and keyboard focus states", () => {
  const onFocus = vi.fn();
  const onToggleSelection = vi.fn();
  const { rerender } = render(
    <PortraitGrid
      items={portraits.slice(0, 1)}
      total={1}
      role="small"
      loading
      queryPending
      assetUrl={(id) => `portrait://thumbnail/${id}/small/160`}
      onFocus={onFocus}
      onToggleSelection={onToggleSelection}
    />,
  );
  expect(screen.getByText("Updating results…")).toBeInTheDocument();
  expect(screen.queryByText("Portrait 0")).not.toBeInTheDocument();

  rerender(
    <PortraitGrid
      items={[]}
      total={0}
      role="small"
      error={new Error("Catalog unavailable")}
      assetUrl={(id) => `portrait://thumbnail/${id}/small/160`}
      onFocus={onFocus}
      onToggleSelection={onToggleSelection}
    />,
  );
  expect(screen.getByRole("alert")).toHaveTextContent("Catalog unavailable");

  rerender(
    <PortraitGrid
      items={[]}
      total={0}
      role="small"
      assetUrl={(id) => `portrait://thumbnail/${id}/small/160`}
      onFocus={onFocus}
      onToggleSelection={onToggleSelection}
    />,
  );
  expect(screen.getByText("No portraits match these filters.")).toBeInTheDocument();

  rerender(
    <PortraitGrid
      items={portraits.slice(0, 1)}
      total={1}
      role="small"
      assetUrl={(id) => `portrait://thumbnail/${id}/small/160`}
      onFocus={onFocus}
      onToggleSelection={onToggleSelection}
    />,
  );
  const card = screen.getByRole("button", { name: /Preview Portrait 0/ });
  card.focus();
  fireEvent.keyDown(card, { key: "Enter" });
  fireEvent.click(card);
  expect(onFocus).toHaveBeenCalledTimes(1);
  expect(onFocus).toHaveBeenCalledWith("portrait-0");
  expect(card).toHaveFocus();
  fireEvent.click(screen.getByRole("checkbox", { name: "Select Portrait 0" }));
  expect(onToggleSelection).toHaveBeenCalledWith("portrait-0", true);
});
