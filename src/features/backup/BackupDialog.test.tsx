import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { vi, it, expect } from "vitest";
import { BackupDialog } from "./BackupDialog";
import type { LibraryApi } from "../../lib/api";
import type { Job } from "../../lib/contracts";
const job: Job = { id: "job", state: "running", completed: 0, total: null, message: "Writing portable backup" };
it("requires confirmation for the exact backup path and exposes cancellation", async () => {
  const startBackup = vi.fn().mockRejectedValueOnce({code:"BACKUP_CONFIRMATION_REQUIRED",message:"Confirm path"}).mockResolvedValue(job);
  const cancelJob = vi.fn().mockResolvedValue(undefined);
  const onClose = vi.fn();
  const api = {startBackup,cancelJob,getJob:vi.fn().mockResolvedValue(job)} as unknown as LibraryApi;
  render(<BackupDialog api={api} mode="backup" onClose={onClose} />);
  const input=screen.getByLabelText("Backup ZIP path");fireEvent.change(input,{target:{value:"/tmp/old.zip"}});fireEvent.click(screen.getByText("Save backup"));
  await screen.findByLabelText("Confirm backup overwrite");expect(screen.getByText("Save backup")).toBeDisabled();
  fireEvent.click(screen.getByLabelText("Confirm backup overwrite"));fireEvent.change(input,{target:{value:"/tmp/new.zip"}});
  expect(screen.queryByLabelText("Confirm backup overwrite")).not.toBeInTheDocument();fireEvent.click(screen.getByText("Save backup"));
  await screen.findByText("Cancel backup");expect(startBackup).toHaveBeenLastCalledWith("/tmp/new.zip",null);
  fireEvent.keyDown(screen.getByRole("dialog"),{key:"Escape"});expect(onClose).not.toHaveBeenCalled();
  fireEvent.click(screen.getByText("Cancel backup"));expect(cancelJob).toHaveBeenCalledWith("job");
});
it("opens the verified restored library after the job completes", async () => {
  const library={id:"same-id",name:"Restored",path:"/tmp/restored"};const onRestored=vi.fn();const onClose=vi.fn();
  const api={startRestore:vi.fn().mockResolvedValue(job), getJob:vi.fn().mockResolvedValue({...job,state:"done",message:"Library restored and verified"}),getRestoreResult:vi.fn().mockResolvedValue(library)} as unknown as LibraryApi;
  render(<BackupDialog api={api} mode="restore" onClose={onClose} onRestored={onRestored} />);
  fireEvent.change(screen.getByLabelText("Backup archive"),{target:{value:"/tmp/backup.zip"}});fireEvent.change(screen.getByLabelText("Restore folder"),{target:{value:"/tmp/restored"}});fireEvent.click(screen.getByText("Restore backup"));
  await waitFor(()=>expect(screen.getByText("Open restored library")).toBeInTheDocument());fireEvent.click(screen.getByText("Open restored library"));
  expect(onRestored).toHaveBeenCalledWith(library);expect(onClose).toHaveBeenCalled();
});
