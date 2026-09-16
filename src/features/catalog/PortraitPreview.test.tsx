import { createEvent, fireEvent, render, screen } from "@testing-library/react";
import { vi } from "vitest";

import { PortraitCard } from "./PortraitCard";
import { PortraitPreview } from "./PortraitPreview";

test("does not select a portrait when opening its preview", () => {
  const onFocus = vi.fn();
  const onToggleSelection = vi.fn();
  const portrait = {
    id: "p",
    sourceId: "s",
    name: "Elf",
    sourceName: "Pack",
    originalFolder: "elf",
    description: null,
    labels: [],
    selected: false,
    trashedAt: null,
  };

  render(
    <PortraitCard
      portrait={portrait}
      role="medium"
      assetUrl={() => "portrait://thumbnail/p/medium/360"}
      onFocus={onFocus}
      onToggleSelection={onToggleSelection}
    />,
  );

  fireEvent.click(screen.getByRole("button", { name: "Preview Elf" }));

  expect(onFocus).toHaveBeenCalledWith("p");
  expect(onToggleSelection).not.toHaveBeenCalled();
});

test("shows fitted role previews and waits to load an original", () => {
  const portrait = {
    id: "p",
    sourceId: "s",
    name: "Elf",
    sourceName: "Pack",
    originalFolder: "elf",
    description: "A patient archer.",
    labels: [{ category: "ancestry", value: "elf" }],
    selected: false,
    trashedAt: null,
  };

  render(
    <PortraitPreview
      portrait={portrait}
      visible
      assetUrl={(id, role, variant) => `portrait:///${variant}/${id}/${role}`}
      onClose={vi.fn()}
      onNavigate={vi.fn()}
    />,
  );

  expect(screen.getByRole("complementary", { name: "Portrait preview" })).toHaveTextContent("Elf");
  expect(screen.getByText("Pack")).toBeInTheDocument();
  expect(screen.getByText("A patient archer.")).toBeInTheDocument();
  expect(screen.getByText("ancestry: elf")).toBeInTheDocument();
  expect(screen.getByRole("img", { name: "Elf small portrait" })).toHaveAttribute("src", "portrait:///thumbnail/p/small");
  expect(screen.getByRole("img", { name: "Elf medium portrait" })).toHaveAttribute("src", "portrait:///thumbnail/p/medium");
  expect(screen.getByRole("img", { name: "Elf large portrait" })).toHaveAttribute("src", "portrait:///thumbnail/p/large");
  expect(screen.queryByAltText("Elf large portrait at native resolution")).not.toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "View Large at native resolution" }));

  expect(screen.getByAltText("Elf large portrait at native resolution")).toHaveAttribute("src", "portrait:///original/p/large");
});

test("closes on Escape and moves through focused portraits with arrow keys", () => {
  const onClose = vi.fn();
  const onNavigate = vi.fn();
  render(
    <PortraitPreview
      portrait={{ id: "p", sourceId: "s", name: "Elf", sourceName: "Pack", originalFolder: "elf", description: null, labels: [], selected: false, trashedAt: null }}
      visible
      assetUrl={() => "portrait:///thumbnail/p/small"}
      onClose={onClose}
      onNavigate={onNavigate}
    />,
  );

  const preview = screen.getByRole("complementary", { name: "Portrait preview" });
  fireEvent.keyDown(preview, { key: "ArrowRight" });
  fireEvent.keyDown(preview, { key: "ArrowLeft" });
  fireEvent.keyDown(preview, { key: "Escape" });

  expect(onNavigate).toHaveBeenNthCalledWith(1, 1);
  expect(onNavigate).toHaveBeenNthCalledWith(2, -1);
  expect(onClose).toHaveBeenCalledOnce();
});

test("keeps the preview open when Escape closes a focused native-resolution dialog", async () => {
  const onClose = vi.fn();
  const onNavigate = vi.fn();
  render(
    <PortraitPreview
      portrait={{ id: "p", sourceId: "s", name: "Elf", sourceName: "Pack", originalFolder: "elf", description: null, labels: [], selected: false, trashedAt: null }}
      visible
      assetUrl={(id, role, variant) => `portrait:///${variant}/${id}/${role}`}
      onClose={onClose}
      onNavigate={onNavigate}
    />,
  );

  const nativeTrigger = screen.getByRole("button", { name: "View Large at native resolution" });
  fireEvent.click(nativeTrigger);
  const dialog = screen.getByRole("dialog", { name: "Large native resolution" });

  expect(screen.getByRole("button", { name: "Close native resolution" })).toHaveFocus();
  fireEvent.keyDown(dialog, { key: "ArrowRight" });
  fireEvent.keyDown(dialog, { key: "Escape" });

  expect(onNavigate).not.toHaveBeenCalled();
  expect(onClose).not.toHaveBeenCalled();
  expect(screen.queryByRole("dialog", { name: "Large native resolution" })).not.toBeInTheDocument();
  expect(nativeTrigger).toHaveFocus();
});

test("contains Tab and Shift+Tab inside the native-resolution dialog", () => {
  const onNavigate = vi.fn();
  render(
    <PortraitPreview
      portrait={{ id: "p", sourceId: "s", name: "Elf", sourceName: "Pack", originalFolder: "elf", description: null, labels: [], selected: false, trashedAt: null }}
      visible
      assetUrl={(id, role, variant) => `portrait:///${variant}/${id}/${role}`}
      onClose={vi.fn()}
      onNavigate={onNavigate}
    />,
  );

  fireEvent.click(screen.getByRole("button", { name: "View Large at native resolution" }));
  const dialog = screen.getByRole("dialog", { name: "Large native resolution" });
  const close = screen.getByRole("button", { name: "Close native resolution" });
  const forwardTab = createEvent.keyDown(dialog, { key: "Tab" });
  fireEvent(dialog, forwardTab);
  const backwardTab = createEvent.keyDown(dialog, { key: "Tab", shiftKey: true });
  fireEvent(dialog, backwardTab);

  expect(forwardTab.defaultPrevented).toBe(true);
  expect(backwardTab.defaultPrevented).toBe(true);
  expect(close).toHaveFocus();
  expect(onNavigate).not.toHaveBeenCalled();
});

test("keeps editor caret keys and Escape inside preview metadata fields", () => {
  const onClose = vi.fn();
  const onNavigate = vi.fn();
  render(
    <PortraitPreview
      portrait={{ id: "p", sourceId: "s", name: "Elf", sourceName: "Pack", originalFolder: "elf", description: null, labels: [], selected: false, trashedAt: null }}
      visible
      assetUrl={() => "portrait:///thumbnail/p/small"}
      onClose={onClose}
      onNavigate={onNavigate}
      editMetadata={vi.fn().mockResolvedValue(undefined)}
    />,
  );

  const description = screen.getByRole("textbox", { name: "Description" });
  fireEvent.keyDown(description, { key: "ArrowRight" });
  fireEvent.keyDown(description, { key: "ArrowLeft" });
  fireEvent.keyDown(description, { key: "Escape" });

  expect(onNavigate).not.toHaveBeenCalled();
  expect(onClose).not.toHaveBeenCalled();
});
