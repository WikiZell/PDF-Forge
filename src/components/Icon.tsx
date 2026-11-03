// Small hand-drawn stroke icon set (24x24 grid). Decorative: always aria-hidden, the button carries the name.
const P: Record<string, string> = {
  open: "M3 7.5A1.5 1.5 0 0 1 4.5 6H9l2 2.2h8.5A1.5 1.5 0 0 1 21 9.7V17a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 17z",
  merge: "M8 6h9a2 2 0 0 1 2 2v9M5 9v9a2 2 0 0 0 2 2h9M8 3h6a2 2 0 0 1 2 2v2H8a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z",
  bulk: "M7 4h8l4 4v10a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2zM15 4v4h4M8.5 15.5c1.2-1.6 2-1.6 2.5 0s1.4 1.6 2.5-.4",
  settings: "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19.4 13.5a7.7 7.7 0 0 0 0-3l1.7-1.3-1.7-3-2 .8a7.6 7.6 0 0 0-2.6-1.5L14.5 3h-3.4l-.3 2.5a7.6 7.6 0 0 0-2.6 1.5l-2-.8-1.7 3 1.7 1.3a7.7 7.7 0 0 0 0 3l-1.7 1.3 1.7 3 2-.8a7.6 7.6 0 0 0 2.6 1.5l.3 2.5h3.4l.3-2.5a7.6 7.6 0 0 0 2.6-1.5l2 .8 1.7-3z",
  organize: "M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z",
  sign: "M4 20h16M5 16.5 15.8 5.7a2 2 0 0 1 2.8 0l-.1.1a2 2 0 0 1 0 2.8L7.7 19.4 4 20z",
  sidebar: "M4 5h16a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1zM9 5v14",
  prev: "M14.5 6 8.5 12l6 6",
  next: "M9.5 6l6 6-6 6",
  minus: "M5 12h14",
  plus: "M12 5v14M5 12h14",
  fitw: "M4 12h16M7 8.5 3.5 12 7 15.5M17 8.5l3.5 3.5-3.5 3.5",
  fitp: "M4 9V5a1 1 0 0 1 1-1h4M20 9V5a1 1 0 0 0-1-1h-4M4 15v4a1 1 0 0 0 1 1h4M20 15v4a1 1 0 0 1-1 1h-4",
  find: "M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13zM20 20l-4.9-4.9",
  print: "M7 9V4h10v5M7 17H5a1 1 0 0 1-1-1v-5a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v5a1 1 0 0 1-1 1h-2M7 14h10v6H7z",
  close: "M6 6l12 12M18 6 6 18",
  pdf: "M7 3h7l5 5v11a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2zM14 3v5h5",
  shield: "M12 3 4.5 6v5.5c0 4.4 3.1 8 7.5 9.5 4.4-1.5 7.5-5.1 7.5-9.5V6z",
};

export function Icon({ name, size = 18 }: { name: keyof typeof P | string; size?: number }) {
  return (
    <svg aria-hidden="true" focusable="false" width={size} height={size} viewBox="0 0 24 24" fill="none"
      stroke="currentColor" strokeWidth={1.75} strokeLinecap="round" strokeLinejoin="round">
      <path d={P[name]} />
    </svg>
  );
}
