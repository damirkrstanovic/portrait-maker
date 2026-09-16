import { useEffect, useRef, useState } from "react";

import type { Role } from "../../lib/contracts";

type Props = {
  id: string;
  name: string;
  role: Role;
  assetUrl(id: string, role: Role, variant: "thumbnail" | "original"): string;
};

const roleName = (role: Role) => role[0].toUpperCase() + role.slice(1);

export function PreviewImage({ id, name, role, assetUrl }: Props) {
  const [showNative, setShowNative] = useState(false);
  const nativeTrigger = useRef<HTMLButtonElement>(null);
  const closeNative = useRef<HTMLButtonElement>(null);
  const wasNativeVisible = useRef(false);
  const title = roleName(role);

  useEffect(() => {
    if (showNative) closeNative.current?.focus();
    if (!showNative && wasNativeVisible.current) nativeTrigger.current?.focus();
    wasNativeVisible.current = showNative;
  }, [showNative]);

  return <section className={`preview-image preview-image-${role}`}>
    <header>
      <h3>{title}</h3>
      <button ref={nativeTrigger} type="button" onClick={() => setShowNative(true)}>View {title} at native resolution</button>
    </header>
    <img src={assetUrl(id, role, "thumbnail")} alt={`${name} ${role} portrait`} loading="lazy" />
    {showNative ? <div className="preview-native" role="dialog" aria-label={`${title} native resolution`} aria-modal="true" onKeyDown={(event) => { event.stopPropagation(); if (event.key === "Tab") { event.preventDefault(); closeNative.current?.focus(); } if (event.key === "Escape") { event.preventDefault(); setShowNative(false); } }}>
      <div className="preview-native-toolbar"><span>{title} at native resolution</span><button ref={closeNative} type="button" onClick={() => setShowNative(false)}>Close native resolution</button></div>
      <div className="preview-native-canvas"><img src={assetUrl(id, role, "original")} alt={`${name} ${role} portrait at native resolution`} /></div>
    </div> : null}
  </section>;
}
