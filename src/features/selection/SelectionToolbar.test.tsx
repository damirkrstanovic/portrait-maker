import { fireEvent, render, screen } from "@testing-library/react";
import { vi } from "vitest";

import { SelectionToolbar } from "./SelectionToolbar";

const query = { text: "elf", sourceIds: [], labels: [], selectedOnly: false, trash: false };

test("adds and removes the frozen matching query, then clears persistent selection", async () => {
  const changeSelection = vi.fn().mockResolvedValueOnce(12).mockResolvedValueOnce(3).mockResolvedValueOnce(0);
  render(<SelectionToolbar query={query} matchingCount={12} selectedCount={0} changeSelection={changeSelection} onChanged={vi.fn()} />);

  fireEvent.click(screen.getByRole("button", { name: "Select all 12 matching" }));
  fireEvent.click(screen.getByRole("button", { name: "Remove 12 matching" }));
  fireEvent.click(screen.getByRole("button", { name: "Clear selection" }));

  await vi.waitFor(() => expect(changeSelection).toHaveBeenNthCalledWith(1, { matching: query }, "add"));
  expect(changeSelection).toHaveBeenNthCalledWith(2, { matching: query }, "remove");
  expect(changeSelection).toHaveBeenNthCalledWith(3, { ids: [] }, "clear");
});
