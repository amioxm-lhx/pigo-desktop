/**
 * Target sites: where an employee may read, and whether this machine holds a
 * signed-in session for it.
 *
 * The sign-in itself happens on the site's own page in its own window. That is
 * deliberate: the owner scans a QR code the site issued to their phone, no
 * password is typed into this app, and nothing here has to know how the site
 * authenticates. What stays behind afterwards is a cookie in that site's
 * partition, which is the same thing a browser would keep.
 */
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  staffSourceChannelKey,
  type StaffCollectedPage,
  type StaffSite,
} from "@pi-desktop/shared";
import { api } from "../../lib/api";
import { useAppStore } from "../../stores/app-store";
import { Badge, Button, Field, Input, Textarea } from "../../components/ui";
import { IconGlobe, IconLogOut, IconPlay, IconShield } from "../../components/icons";
import { errorText } from "./staff-format";

export function StaffSitePanel() {
  const { t } = useTranslation();
  const showToast = useAppStore((s) => s.showToast);
  const [sites, setSites] = useState<StaffSite[]>([]);
  const [selected, setSelected] = useState<string>("");
  const [seedUrl, setSeedUrl] = useState("");
  const [maxUrls, setMaxUrls] = useState("1");
  const [keywords, setKeywords] = useState("");
  const [busy, setBusy] = useState(false);
  const [pages, setPages] = useState<StaffCollectedPage[]>([]);

  const load = async () => {
    try {
      const res = await api.listStaffSites();
      const list = res.sites ?? [];
      setSites(list);
      setSelected((current) => current || list.find((site) => site.supported)?.channel || list[0]?.channel || "");
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const site = sites.find((entry) => entry.channel === selected) ?? null;

  useEffect(() => {
    setSeedUrl(site?.seedUrl ?? "");
    setMaxUrls(String(site?.maxUrls ?? 1));
    setKeywords((site?.keywords ?? []).join("\n"));
    setPages([]);
  }, [site?.channel, site?.seedUrl, site?.maxUrls, site?.keywords]);

  const signIn = async () => {
    if (!site) return;
    setBusy(true);
    showToast(t("staff.sites.windowOpened"));
    try {
      const result = await api.signInStaffSite(site.channel);
      showToast(result.signedIn ? t("staff.sites.signedInToast") : t("staff.sites.cancelled"));
      await load();
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    } finally {
      setBusy(false);
    }
  };

  const signOut = async () => {
    if (!site) return;
    setBusy(true);
    try {
      await api.signOutStaffSite(site.channel);
      setPages([]);
      await load();
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    } finally {
      setBusy(false);
    }
  };

  const save = async () => {
    if (!site) return;
    setBusy(true);
    try {
      await api.updateStaffSite(site.channel, {
        seedUrl: seedUrl.trim(),
        maxUrls: Number(maxUrls) || 1,
        keywords: keywords
          .split("\n")
          .map((line) => line.trim())
          .filter(Boolean),
      });
      await load();
      showToast(t("staff.sites.saved"));
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    } finally {
      setBusy(false);
    }
  };

  const collect = async () => {
    if (!site) return;
    setBusy(true);
    try {
      const res = await api.collectStaffSite(site.channel);
      setPages(res.pages ?? []);
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="staff-body">
      <div className="staff-list">
        {sites.map((entry) => (
          <button
            key={entry.channel}
            type="button"
            className={"staff-list-item" + (entry.channel === selected ? " on" : "")}
            onClick={() => setSelected(entry.channel)}
          >
            <IconGlobe size={13} aria-hidden />
            <span className="staff-list-main">
              <span>{t(staffSourceChannelKey(entry.channel))}</span>
              <span className="staff-list-sub">
                {entry.supported ? entry.channel : t("staff.sites.unsupported")}
              </span>
            </span>
            <Badge
              tone={!entry.supported ? "warning" : entry.signedIn ? "success" : "warning"}
            >
              {!entry.supported
                ? t("staff.sites.unsupported")
                : entry.signedIn
                  ? t("staff.sites.signedIn")
                  : t("staff.sites.signedOut")}
            </Badge>
          </button>
        ))}
      </div>

      <div className="staff-form">
        <div className="staff-section-title">
          {site ? t(staffSourceChannelKey(site.channel)) : t("staff.sites.title")}
        </div>
        <div className="staff-hint">{t("staff.sites.hint")}</div>
        {site && !site.supported ? (
          <div className="staff-hint">{t("staff.sites.unsupportedHint")}</div>
        ) : null}
        {site && site.supported && !site.signedIn ? (
          <div className="staff-hint staff-hint-warn">{t("staff.sites.signInFirst")}</div>
        ) : null}
        {site && site.supported ? (
          <>
            <Field label={t("staff.sites.keywords")} hint={t("staff.sites.keywordsHint")}>
              <Textarea
                rows={6}
                value={keywords}
                placeholder={site.keywords.join("\n")}
                onChange={(event) => setKeywords(event.target.value)}
              />
            </Field>
            <div className="staff-grid">
              <Field label={t("staff.sites.seedUrl")} hint={t("staff.sites.seedUrlHint")}>
                <Input
                  value={seedUrl}
                  placeholder={site.seedUrl}
                  onChange={(event) => setSeedUrl(event.target.value)}
                />
              </Field>
              <Field label={t("staff.sites.maxUrls")} hint={t("staff.sites.maxUrlsHint")}>
                <Input
                  value={maxUrls}
                  inputMode="numeric"
                  onChange={(event) => setMaxUrls(event.target.value)}
                />
              </Field>
            </div>
            <div className="staff-toolbar">
              <Button variant="primary" disabled={busy} onClick={() => void signIn()}>
                <IconShield size={13} aria-hidden />
                {site.signedIn ? t("staff.sites.signInAgain") : t("staff.sites.signIn")}
              </Button>
              <Button variant="ghost" disabled={busy} onClick={() => void save()}>
                {t("staff.sites.save")}
              </Button>
              <Button variant="ghost" disabled={busy || !site.signedIn} onClick={() => void collect()}>
                <IconPlay size={13} aria-hidden />
                {t("staff.sites.collectNow")}
              </Button>
              <Button variant="ghost" disabled={busy || !site.signedIn} onClick={() => void signOut()}>
                <IconLogOut size={13} aria-hidden />
                {t("staff.sites.signOut")}
              </Button>
            </div>

            {pages.length > 0 ? (
              <div className="staff-collected">
                {pages.map((page) => (
                  <div key={page.url} className="staff-collected-page">
                    <div className="staff-collected-head">
                      <b>{page.title || page.url}</b>
                      <span className="staff-hint">
                        {page.error
                          ? page.error
                          : t("staff.sites.itemCount", { count: page.itemCount })}
                      </span>
                    </div>
                    {page.items.slice(0, 8).map((item) => (
                      <div key={item.title} className="staff-collected-item">
                        {item.title}
                      </div>
                    ))}
                  </div>
                ))}
              </div>
            ) : null}
          </>
        ) : null}
      </div>
    </div>
  );
}
