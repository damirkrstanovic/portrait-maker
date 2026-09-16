import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";

import type { Destination } from "../../lib/contracts";
import { DestinationList } from "./DestinationList";

const existing: Destination = {
  id: "wotr-prefix", game: "wotr", name: "Pathfinder: Wrath of the Righteous — Heroic", path: "/games/wotr/Portraits",
  origin: "manual", state: "existing", evidence: ["Manual compatibility prefix user: heroic-user"],
};

test("imports an existing destination through the normal game import command", () => {
  const onImportExisting = vi.fn();
  render(<DestinationList destinations={[existing]} busy={false} onRefresh={() => {}} onAdd={() => {}} onImportExisting={onImportExisting} />);

  fireEvent.click(screen.getByRole("button", { name: "Import existing portraits from Pathfinder: Wrath of the Righteous — Heroic" }));
  expect(onImportExisting).toHaveBeenCalledWith(existing);
  expect(screen.getByText("Manual compatibility prefix user: heroic-user")).toBeInTheDocument();
});

test("does not offer import for a destination without a Portraits directory", () => {
  render(<DestinationList destinations={[{ ...existing, state: "missingPortraits" }]} busy={false} onRefresh={() => {}} onAdd={() => {}} onImportExisting={() => {}} />);
  expect(screen.queryByRole("button", { name: /Import existing portraits/ })).not.toBeInTheDocument();
  expect(screen.getByText("Portraits folder is absent")).toBeInTheDocument();
});
