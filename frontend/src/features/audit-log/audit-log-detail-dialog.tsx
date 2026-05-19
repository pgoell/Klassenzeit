import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { type AuditLogDetail, useAuditLogDetail } from "./hooks";

type Props = {
  auditLogId: string | null;
  onClose: () => void;
};

export function AuditLogDetailDialog({ auditLogId, onClose }: Props) {
  const { t } = useTranslation();
  const query = useAuditLogDetail(auditLogId);
  const open = auditLogId !== null;

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
    >
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t("auditLog.detail.title")}</DialogTitle>
          <DialogDescription>{t("auditLog.detail.subtitle")}</DialogDescription>
        </DialogHeader>
        {query.isLoading ? (
          <p className="text-sm text-muted-foreground">{t("auditLog.detail.loading")}</p>
        ) : query.isError || !query.data ? (
          <p className="text-sm text-destructive" role="alert">
            {t("auditLog.errors.detailFetchFailed")}
          </p>
        ) : (
          <DetailBody detail={query.data} />
        )}
      </DialogContent>
    </Dialog>
  );
}

function DetailBody({ detail }: { detail: AuditLogDetail }) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-1 text-sm">
        <span className="font-medium">{t("auditLog.detail.requestId")}</span>
        <span className="font-mono text-xs text-muted-foreground">{detail.request_id ?? "—"}</span>
      </div>
      {detail.request_body_truncated && (
        <p
          className="rounded-md border border-dashed bg-muted px-3 py-2 text-xs text-muted-foreground"
          role="status"
        >
          {t("auditLog.detail.truncated")}
        </p>
      )}
      <JsonBlock label={t("auditLog.detail.pathParams")} value={detail.path_params} />
      {detail.request_body === null ? (
        <div className="flex flex-col gap-1">
          <span className="text-sm font-medium">{t("auditLog.detail.requestBody")}</span>
          <p className="text-sm italic text-muted-foreground">{t("auditLog.detail.noBody")}</p>
        </div>
      ) : (
        <JsonBlock label={t("auditLog.detail.requestBody")} value={detail.request_body} />
      )}
    </div>
  );
}

function JsonBlock({ label, value }: { label: string; value: unknown }) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  const text = JSON.stringify(value, null, 2);
  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center justify-between">
        <span className="text-sm font-medium">{label}</span>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => {
            void navigator.clipboard
              .writeText(text)
              .then(() => {
                setCopied(true);
                window.setTimeout(() => setCopied(false), 1500);
              })
              .catch(() => {
                /* clipboard unavailable; no-op (jsdom + non-HTTPS) */
              });
          }}
        >
          {copied ? t("auditLog.detail.copied") : t("auditLog.detail.copy")}
        </Button>
      </div>
      <pre className="max-h-72 overflow-auto rounded-md border bg-muted p-3 font-mono text-xs whitespace-pre-wrap">
        {text}
      </pre>
    </div>
  );
}
