import { fireEvent, render, screen } from "@testing-library/react";
import { vi } from "vitest";

import { TrashView } from "./TrashView";

const portraits = [
  { id: "elf", sourceId: "source", name: "Elf", sourceName: "Pack", originalFolder: "elf", description: null, labels: [], selected: false, trashedAt: "2026-09-16T10:00:00Z" },
  { id: "dwarf", sourceId: "source", name: "Dwarf", sourceName: "Pack", originalFolder: "dwarf", description: null, labels: [], selected: false, trashedAt: "2026-09-16T10:00:00Z" },
];

test("restores the locally marked trash items and requires a count-specific purge confirmation", () => {
  const restore = vi.fn();
  const purge = vi.fn();
  const toggle = vi.fn();
  render(<TrashView total={2} emptyCount={2} markedIds={new Set(["elf"])} onRestore={restore} onPurge={purge} onEmpty={vi.fn()} onCancelPurge={toggle} />);

  fireEvent.click(screen.getByRole("button", { name: "Restore 1 marked portrait" }));
  expect(restore).toHaveBeenCalledWith(["elf"]);
  fireEvent.click(screen.getByRole("button", { name: "Purge 1 marked portrait" }));
  const dialog = screen.getByRole("dialog", { name: "Permanently delete portraits" });
  expect(dialog).toHaveTextContent("Permanently delete 1 portrait");
  fireEvent.click(screen.getByRole("button", { name: "Permanently delete 1 portrait" }));
  expect(purge).toHaveBeenCalledWith(["elf"]);
});

test("empty trash confirmation covers the full filtered trash count and traps Escape", () => {
  const purge = vi.fn();
  render(<TrashView total={2} emptyCount={2} markedIds={new Set()} onRestore={vi.fn()} onPurge={purge} onEmpty={purge} onCancelPurge={vi.fn()} />);

  const trigger = screen.getByRole("button", { name: "Empty trash" });
  trigger.focus();
  fireEvent.click(trigger);
  const dialog = screen.getByRole("dialog", { name: "Permanently delete portraits" });
  expect(dialog).toHaveTextContent("Permanently delete 2 portraits");
  fireEvent.keyDown(dialog, { key: "Escape" });
  expect(screen.queryByRole("dialog", { name: "Permanently delete portraits" })).not.toBeInTheDocument();
  expect(trigger).toHaveFocus();
  expect(purge).not.toHaveBeenCalled();
});

test("Escape returns focus to the marked-purge trigger", () => {
  render(<TrashView total={2} emptyCount={2} markedIds={new Set(["elf"])} onRestore={vi.fn()} onPurge={vi.fn()} onEmpty={vi.fn()} onCancelPurge={vi.fn()} />);
  const trigger = screen.getByRole("button", { name: "Purge 1 marked portrait" });
  trigger.focus();
  fireEvent.click(trigger);
  fireEvent.keyDown(screen.getByRole("dialog", { name: "Permanently delete portraits" }), { key: "Escape" });
  expect(trigger).toHaveFocus();
});

test("marked IDs remain authoritative when the visible page changes", () => {
  const restore = vi.fn();
  const marked = new Set(["elf", "dwarf"]);
  render(<TrashView total={1} emptyCount={2} markedIds={marked} onRestore={restore} onPurge={vi.fn()} onEmpty={vi.fn()} onCancelPurge={vi.fn()} />);

  fireEvent.click(screen.getByRole("button", { name: "Restore 2 marked portraits" }));
  expect(restore).toHaveBeenCalledWith(["elf", "dwarf"]);
});

test("shows a running purge job and forwards its cancellation control", () => {
  const cancel = vi.fn();
  render(<TrashView total={1} emptyCount={1} markedIds={new Set(["elf"])} purgeJob={{ id: "purge", state: "running", completed: 1, total: 2, message: "Permanently deleting 1 portrait" }} onRestore={vi.fn()} onPurge={vi.fn()} onEmpty={vi.fn()} onCancelPurge={cancel} />);

  expect(screen.getByText("Permanently deleting 1 portrait")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Purge 1 marked portrait" })).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "Cancel deletion" }));
  expect(cancel).toHaveBeenCalledOnce();
});
