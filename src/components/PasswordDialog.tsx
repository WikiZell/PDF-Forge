import { useState } from "react";
import { useModal } from "../hooks/useModal";

interface Props {
  fileName: string;
  wrong: boolean;
  onSubmit: (password: string) => void;
  onCancel: () => void;
}

/** The password lives only in this component's state and is dropped on close. */
export function PasswordDialog({ fileName, wrong, onSubmit, onCancel }: Props) {
  const [pw, setPw] = useState("");
  const dlg = useModal<HTMLFormElement>(onCancel, "#pw");

  return (
    <div className="fixed inset-0 z-10 grid place-items-center bg-black/40" role="presentation">
      <form
        ref={dlg}
        role="dialog"
        aria-modal="true"
        aria-labelledby="pw-title"
        className="w-96 rounded-lg bg-white p-5 shadow-xl dark:bg-neutral-800"
        onSubmit={(e) => {
          e.preventDefault();
          onSubmit(pw);
        }}
      >
        <h2 id="pw-title" className="mb-1 text-lg font-semibold">Password required</h2>
        <p className="mb-3 break-all text-sm text-neutral-600 dark:text-neutral-300">{fileName}</p>
        <label className="block text-sm" htmlFor="pw">Password</label>
        <input
          id="pw"
          type="password"
          autoComplete="off"
          value={pw}
          onChange={(e) => setPw(e.target.value)}
          className="mt-1 w-full rounded border px-2 py-1 dark:bg-neutral-900"
          aria-invalid={wrong}
          aria-describedby={wrong ? "pw-err" : undefined}
        />
        {wrong && (
          <p id="pw-err" role="alert" className="mt-1 text-sm text-red-600">
            The password is incorrect.
          </p>
        )}
        <div className="mt-4 flex justify-end gap-2">
          <button type="button" className="rounded border px-3 py-1" onClick={onCancel}>Cancel</button>
          <button type="submit" className="rounded bg-blue-600 px-3 py-1 text-white">Open</button>
        </div>
      </form>
    </div>
  );
}
