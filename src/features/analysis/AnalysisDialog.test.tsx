import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import type { LibraryApi } from "../../lib/api";
import type { Job } from "../../lib/contracts";
import { AnalysisDialog } from "./AnalysisDialog";

const settings = { endpoint: "http://127.0.0.1:8080/v1/chat/completions", model: "gemma-4-26b-a4b", apiKeyPath: "/private/.apikey" };
const running: Job = { id: "analysis", state: "running", completed: 0, total: 3, message: "Analyzing portraits" };

test("starts an explicit selected-only batch and refreshes the catalog after completion", async () => {
  const startAnalysis = vi.fn().mockResolvedValue(running);
  const onChanged = vi.fn();
  const api = { getAnalysisSettings: vi.fn().mockResolvedValue(settings), startAnalysis, getJob: vi.fn().mockResolvedValue({ ...running, state: "done", completed: 3, message: "Analyzed 3 portraits" }) } as unknown as LibraryApi;
  render(<AnalysisDialog api={api} visible onClose={vi.fn()} onChanged={onChanged} />);
  await waitFor(() => expect(screen.getByRole("button", { name: "Start analysis" })).toBeEnabled());
  expect(startAnalysis).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Start analysis" }));
  await screen.findByText("Analyzed 3 portraits");
  expect(startAnalysis).toHaveBeenCalledWith({ ...settings, selectedOnly: true, overwrite: false });
  expect(onChanged).toHaveBeenCalledTimes(1);
});

test("allows whole-library reanalysis and keeps the running job when the dialog is hidden", async () => {
  const api = { getAnalysisSettings: vi.fn().mockResolvedValue(settings), startAnalysis: vi.fn().mockResolvedValue(running), getJob: vi.fn().mockResolvedValue(running), cancelJob: vi.fn().mockResolvedValue(undefined) } as unknown as LibraryApi;
  const onChanged = vi.fn(), onClose = vi.fn();
  const { rerender } = render(<AnalysisDialog api={api} visible onClose={onClose} onChanged={onChanged} />);
  await waitFor(() => expect(screen.getByRole("button", { name: "Start analysis" })).toBeEnabled());
  fireEvent.click(screen.getByLabelText("All active portraits"));
  fireEvent.click(screen.getByLabelText("Reanalyze portraits that already have a model description"));
  fireEvent.click(screen.getByRole("button", { name: "Start analysis" }));
  await screen.findByRole("button", { name: "Stop analysis" });
  expect(api.startAnalysis).toHaveBeenCalledWith({ ...settings, selectedOnly: false, overwrite: true });
  fireEvent.click(screen.getByRole("button", { name: "Run in background" }));
  expect(onClose).toHaveBeenCalled();
  rerender(<AnalysisDialog api={api} visible={false} onClose={onClose} onChanged={onChanged} />);
  expect(screen.queryByRole("dialog")).toBeNull();
  rerender(<AnalysisDialog api={api} visible onClose={onClose} onChanged={onChanged} />);
  fireEvent.click(screen.getByRole("button", { name: "Stop analysis" }));
  await waitFor(() => expect(api.cancelJob).toHaveBeenCalledWith("analysis"));
  expect(screen.getByText(/Stopping after the current request/)).toBeInTheDocument();
});
