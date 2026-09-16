import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { vi } from "vitest";

import { MetadataEditor } from "./MetadataEditor";

const portrait = {
  id: "elf",
  sourceId: "pack",
  name: "Elf",
  sourceName: "Companions",
  originalFolder: "elf",
  description: null,
  labels: [],
  selected: true,
  trashedAt: null,
};

test("saves a portrait description and custom label without letting Escape escape the editor", async () => {
  const editMetadata = vi.fn().mockResolvedValue(undefined);
  render(<MetadataEditor portraits={[portrait]} editMetadata={editMetadata} />);

  fireEvent.change(screen.getByRole("textbox", { name: "Description" }), { target: { value: " Watches the wardstone. " } });
  fireEvent.change(screen.getByRole("textbox", { name: "Label category" }), { target: { value: "Role" } });
  fireEvent.change(screen.getByRole("textbox", { name: "Label value" }), { target: { value: "Watcher" } });
  fireEvent.click(screen.getByRole("button", { name: "Add label" }));
  fireEvent.click(screen.getByRole("button", { name: "Save metadata" }));

  expect(editMetadata).toHaveBeenCalledWith(["elf"], {
    name: undefined,
    description: " Watches the wardstone. ",
    addLabels: [{ category: "Role", value: "Watcher" }],
    removeLabels: [],
  });
  const escape = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
  screen.getByRole("textbox", { name: "Description" }).dispatchEvent(escape);
  expect(escape.defaultPrevented).toBe(true);
});

test("applies and clears an explicitly edited description for every selected portrait", async () => {
  const second = { ...portrait, id: "dwarf", name: "Dwarf" };
  const editMetadata = vi.fn().mockResolvedValue(undefined);
  const view = render(<MetadataEditor portraits={[portrait, second]} editMetadata={editMetadata} />);

  const description = screen.getByRole("textbox", { name: "Description" });
  fireEvent.change(description, { target: { value: "Shared history" } });
  fireEvent.click(screen.getByRole("button", { name: "Save metadata" }));
  await waitFor(() => expect(editMetadata).toHaveBeenLastCalledWith(["elf", "dwarf"], {
    name: undefined,
    description: "Shared history",
    addLabels: [],
    removeLabels: [],
  }));

  view.unmount();
  render(<MetadataEditor portraits={[portrait, second]} editMetadata={editMetadata} />);
  const clearing = screen.getByRole("textbox", { name: "Description" });
  fireEvent.change(clearing, { target: { value: "will clear" } });
  fireEvent.change(clearing, { target: { value: "" } });
  fireEvent.click(screen.getByRole("button", { name: "Save metadata" }));
  await waitFor(() => expect(editMetadata).toHaveBeenLastCalledWith(["elf", "dwarf"], {
    name: undefined,
    description: "",
    addLabels: [],
    removeLabels: [],
  }));
});
