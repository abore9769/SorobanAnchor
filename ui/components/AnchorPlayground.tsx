import { useState } from "react";
import { useAnchorPlayground, SEP_PROTOCOLS } from "../hooks/useAnchorPlayground";
import { AnchorPlaygroundHeader } from "./AnchorPlaygroundHeader";
import { AnchorPlaygroundOperationSelector } from "./AnchorPlaygroundOperationSelector";
import { AnchorPlaygroundRequestBuilder } from "./AnchorPlaygroundRequestBuilder";
import { AnchorPlaygroundResponseViewer } from "./AnchorPlaygroundResponseViewer";

export default function AnchorPlayground() {
  const [dark, setDark] = useState(true);
  const pg = useAnchorPlayground();

  return (
    <div
      data-testid="anchor-playground"
      style={{
        fontFamily: "'JetBrains Mono','Fira Code',monospace",
        minHeight: "100vh",
        background: dark ? "#050810" : "#f0f4f8",
        color: dark ? "#c8d8ee" : "#1a2a3a",
        padding: "24px",
      }}
    >
      <AnchorPlaygroundHeader
        domain={pg.domain}
        onDomainChange={pg.setDomain}
        dark={dark}
        onToggleDark={() => setDark(d => !d)}
      />

      <div style={{ display: "grid", gridTemplateColumns: "280px 1fr 1fr", gap: 16, marginTop: 16 }}>
        <AnchorPlaygroundOperationSelector
          protocols={SEP_PROTOCOLS}
          activeSEP={pg.activeSEP}
          activeEp={pg.activeEp}
          onSelectSEP={pg.selectSEP}
          onSelectEp={pg.selectEp}
        />

        <AnchorPlaygroundRequestBuilder
          endpoint={pg.activeEp}
          params={pg.params}
          jwt={pg.jwt}
          url={pg.buildUrl()}
          loading={pg.loading}
          onParamChange={pg.setParam}
          onJwtChange={pg.setJwt}
          onSend={pg.sendRequest}
        />

        <AnchorPlaygroundResponseViewer
          response={pg.response}
          error={pg.error}
          history={pg.history}
          copied={pg.copied}
          onCopy={pg.copyResponse}
        />
      </div>
    </div>
  );
}
