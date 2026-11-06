use super::*;
use serde_json::json;

/// What the middle managers read, and how they read it.
///
/// Every proposal here is derived from rows that already exist -- cards nobody
/// moved, drafts nobody approved, replies nobody filed, a topic that is
/// outrunning the others. Nothing is inferred from prose and nothing is
/// invented: if the boards are empty, this returns nothing, which is the
/// honest answer for a business with no evidence yet.
///
/// Deriving is idempotent by (board, title) while a proposal is still open, so
/// the console can ask on every render without stacking duplicates. Once the
/// owner answers one it stops being derived, and if the condition comes back
/// the board is free to raise it again -- the same problem returning is news,
/// and a dismissed proposal must not silence it forever.

/// How many cards a single proposal names before the rest are a count.
const EVIDENCE_LIMIT: usize = 5;

/// A card nobody has touched this long is the day's first thing to raise.
const COLD_LEAD_HOURS: i64 = 12;

/// How far back the topic ranking reads.
const TOPIC_WINDOW_DAYS: i64 = 7;

/// The window a mid-ranking topic has to beat the runner-up by to be worth
/// naming as the direction. Below this the two are simply both working.
const TOPIC_LEAD_RATIO: f64 = 1.5;

/// How stale a shared playbook has to be before the arsenal says so.
const STALE_PLAYBOOK_DAYS: i64 = 14;

fn employee_name(db: &Database, id: &str) -> Option<String> {
    db.conn()
        .query_row(
            "SELECT name FROM employees WHERE id = ?1",
            params![id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten()
}

/// The id of the employee holding a name, for a proposal that names its worker.
///
/// By name rather than by id because the blueprint names are what the seed
/// writes and what the owner recognises; an owner who renamed or deleted that
/// employee gets a proposal with no worker rather than work sent to nobody.
fn employee_id(db: &Database, name: &str) -> Option<String> {
    db.conn()
        .query_row(
            "SELECT id FROM employees WHERE name = ?1",
            params![name],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten()
}

/// The receptionist, by the name the roster hires it under.
const RECEPTIONIST: &str = "接线员";
/// The writer, by the name the roster hires it under.
const WRITER: &str = "笔杆子";
/// The one who carries a warm conversation to money.
const CLOSER: &str = "跟单员";

/// The work order a proposal becomes once the owner accepts it.
///
/// Written here rather than composed at decision time because the board that
/// read the rows is the one that knows whose job the follow-up is. A plan that
/// names nobody is deliberate: approving a draft and rewriting the shared
/// wording are owner acts by construction (see the two doors in the spec), and
/// that wording must not be handed to whichever employee filed the evidence.
fn plan_for(db: &Database, worker: Option<&str>, verb: &str) -> Value {
    match worker {
        Some(name) => json!({
            "employeeId": employee_id(db, name),
            "employeeName": name,
            "verb": verb,
        }),
        None => json!({ "employeeId": null, "employeeName": "", "verb": verb }),
    }
}

/// The name of whichever employee filed a row, for the citation.
fn filer(db: &Database, employee_id: Option<&str>) -> Option<String> {
    employee_id.and_then(|id| employee_name(db, id))
}

/// A count plus the ids behind it, so a proposal can name what it counts.
fn cited(db: &Database, sql: &str, args: &[&dyn rusqlite::ToSql]) -> Result<i64> {
    let mut stmt = db.conn().prepare(sql)?;
    Ok(stmt.query_row(args, |row| row.get::<_, i64>(0))?)
}

/// Which of these titles the board has already raised and not answered.
fn already_open(db: &Database, board: &str, title: &str) -> Result<bool> {
    Ok(db
        .conn()
        .query_row(
            "SELECT 1 FROM staff_suggestions WHERE board = ?1 AND title = ?2 AND status = 'open' LIMIT 1",
            params![board, title],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some())
}

fn propose(db: &Database, input: &Value) -> Result<Option<Suggestion>> {
    let board = required_str(input, "board")?;
    let title = required_str(input, "title")?;
    if already_open(db, &board, &title)? {
        return Ok(None);
    }
    Ok(Some(create_suggestion(db, input)?))
}

/// Cards filed and never touched.
fn cold_leads(db: &Database) -> Result<Option<Suggestion>> {
    let cutoff = now_ms() - COLD_LEAD_HOURS * 3_600_000;
    let waiting = cited(
        db,
        "SELECT COUNT(*) FROM staff_leads
          WHERE stage IN ('new', 'qualified') AND owner_id IS NULL AND created_at <= ?1",
        &[&cutoff],
    )?;
    if waiting == 0 {
        return Ok(None);
    }
    let hottest = cited(
        db,
        "SELECT COUNT(*) FROM staff_leads
          WHERE stage IN ('new', 'qualified') AND owner_id IS NULL AND created_at <= ?1
            AND score >= 70",
        &[&cutoff],
    )?;
    let mut stmt = db.conn().prepare(
        "SELECT id, title, site, quote, score, employee_id, run_id
           FROM staff_leads
          WHERE stage IN ('new', 'qualified') AND owner_id IS NULL AND created_at <= ?1
          ORDER BY score DESC, created_at
          LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![cutoff, EVIDENCE_LIMIT as i64], |row| {
        Ok((
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
        ))
    })?;
    let mut evidence = Vec::new();
    for row in rows {
        let (title, site, quote, score, employee_id, run_id) = row?;
        evidence.push(Evidence {
            employee_name: filer(db, employee_id.as_deref()),
            employee_id,
            run_id,
            step: None,
            label: format!("{title} · {site} · 意向 {score}"),
            quote: Some(quote).filter(|text| !text.is_empty()),
        });
    }
    let body = if hottest > 0 {
        format!(
            "{waiting} 张卡片超过 {COLD_LEAD_HOURS} 小时没人碰，其中 {hottest} 张意向分在 70 以上。意向分是侦察员按原话里有没有具体场景、有没有明确卡点给的，不是流量。"
        )
    } else {
        format!("{waiting} 张卡片超过 {COLD_LEAD_HOURS} 小时没人碰，但都不算高意向。")
    };
    propose(
        db,
        &json!({
            "board": "leads",
            "title": format!("把 {waiting} 张没人碰的卡挑一遍"),
            "body": body,
            "action": "把值得跟的标为待触达并指定负责人，其余归档",
            "directive": plan_for(
                db,
                Some(RECEPTIONIST),
                "把这几张冷卡片按分诊标准过一遍，值得跟的标为待触达并写明切入角度，其余标成不适合",
            ),
            "impact": format!("{hottest} 张高意向卡进入触达队列"),
            "costNote": "占用地推员一次投放（当前 2 次/天），不额外花钱",
            "evidence": evidence,
        }),
    )
}

/// People who answered and are waiting for the next move.
fn unanswered_replies(db: &Database) -> Result<Option<Suggestion>> {
    let waiting = cited(
        db,
        "SELECT COUNT(*) FROM staff_leads WHERE stage = 'replied'",
        &[],
    )?;
    if waiting == 0 {
        return Ok(None);
    }
    let mut stmt = db.conn().prepare(
        "SELECT title, site, quote, replied_at, employee_id, run_id
           FROM staff_leads
          WHERE stage = 'replied'
          ORDER BY replied_at
          LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![EVIDENCE_LIMIT as i64], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<i64>>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
        ))
    })?;
    let mut evidence = Vec::new();
    for row in rows {
        let (title, site, quote, replied_at, employee_id, run_id) = row?;
        let waited = replied_at
            .map(|ms| (now_ms() - ms) / 3_600_000)
            .unwrap_or_default();
        evidence.push(Evidence {
            employee_name: filer(db, employee_id.as_deref()),
            employee_id,
            run_id,
            step: None,
            label: format!("{title} · {site} · 已等 {waited} 小时"),
            quote: Some(quote).filter(|text| !text.is_empty()),
        });
    }
    propose(
        db,
        &json!({
            "board": "leads",
            "title": format!("{waiting} 个人回了话，还没人接"),
            "body": "回了话就说明开场那句问对了，但接不接得住是另一回事：同一个人隔一天再收到回复，热度和第一次完全不同。",
            "action": "把这几位移交给跟单员，按各自卡点写一条推进话术",
            "directive": plan_for(
                db,
                Some(CLOSER),
                "接住这几个已经回话的人，按各自卡点写一条推进话术，卡在哪一步记下来",
            ),
            "impact": format!("{waiting} 段已经热起来的对话进入成交队列"),
            "costNote": "占用跟单员一次运行（当前 2 次/天）",
            "evidence": evidence,
        }),
    )
}

/// Drafts waiting for the owner's yes.
///
/// Both open statuses count. A run files a draft at `draft` or `review` and
/// neither of them goes anywhere without the owner, so treatment that watched
/// only `review` would leave every draft a run happened to file at `draft`
/// sitting on the board with nothing asking about it.
fn drafts_waiting(db: &Database) -> Result<Option<Suggestion>> {
    let waiting = cited(
        db,
        "SELECT COUNT(*) FROM staff_contents WHERE status IN ('draft', 'review')",
        &[],
    )?;
    if waiting == 0 {
        return Ok(None);
    }
    let mut stmt = db.conn().prepare(
        "SELECT title, channel, variant, employee_id, run_id
           FROM staff_contents
          WHERE status IN ('draft', 'review')
          ORDER BY created_at
          LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![EVIDENCE_LIMIT as i64], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;
    let mut evidence = Vec::new();
    for row in rows {
        let (title, channel, variant, employee_id, run_id) = row?;
        let slot = if variant.is_empty() {
            String::new()
        } else {
            format!(" · {variant} 版")
        };
        evidence.push(Evidence {
            employee_name: filer(db, employee_id.as_deref()),
            employee_id,
            run_id,
            step: None,
            label: format!("{title} · {channel}{slot}"),
            quote: None,
        });
    }
    let approved = cited(
        db,
        "SELECT COUNT(*) FROM staff_contents WHERE status = 'approved'",
        &[],
    )?;
    let tail = if approved > 0 {
        format!("另有 {approved} 篇已定稿但还没发出去。")
    } else {
        String::new()
    };
    propose(
        db,
        &json!({
            "board": "contents",
            "title": format!("{waiting} 篇稿子等你定稿"),
            "body": format!("稿子放在那里不会自己变成留言。同题多版是有意写的备选，挑一版定稿比全部留着更有用。{tail}"),
            "action": "逐篇读一遍，挑一版定稿，其余留作下一轮备选",
            "directive": plan_for(
                db,
                None,
                "逐篇读一遍，挑一版定稿，其余留作下一轮备选",
            ),
            "impact": format!("{waiting} 篇进入可投放状态"),
            "costNote": "占用你 5 到 10 分钟，不额外花钱",
            "evidence": evidence,
        }),
    )
}

/// The topic that is outrunning the others.
fn leading_topic(db: &Database) -> Result<Option<Suggestion>> {
    let since = local_day(now_ms() - (TOPIC_WINDOW_DAYS - 1) * 86_400_000);
    let mut stmt = db.conn().prepare(
        "SELECT label, COALESCE(SUM(views), 0) v, COALESCE(SUM(replies), 0) r,
                COALESCE(SUM(leads), 0) l, COALESCE(SUM(inquiries), 0) q,
                COALESCE(SUM(deals), 0) d
           FROM staff_metrics
          WHERE dimension = 'topic' AND day >= ?1
          GROUP BY label
          HAVING SUM(views) + SUM(replies) > 0
          ORDER BY SUM(views) DESC, SUM(replies) DESC
          LIMIT 3",
    )?;
    let rows = stmt.query_map(params![since], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)?,
        ))
    })?;
    let ranked = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    let Some((top, top_views, top_replies, _leads, top_inquiries, top_deals)) =
        ranked.first().cloned()
    else {
        return Ok(None);
    };
    let runner_up = ranked.get(1).cloned();
    let ratio = match &runner_up {
        Some((_, views, replies, _, _, _)) => {
            let other = (*views + *replies).max(1) as f64;
            (top_views + top_replies) as f64 / other
        }
        None => f64::INFINITY,
    };
    if ratio < TOPIC_LEAD_RATIO {
        return Ok(None);
    }
    let mut evidence = vec![Evidence {
        employee_name: None,
        employee_id: None,
        run_id: None,
        step: None,
        label: format!("「{top}」{TOPIC_WINDOW_DAYS} 天 · {top_views} 次浏览 · {top_replies} 次回应"),
        quote: runner_up.map(|(other, views, replies, _, _, _)| {
            format!("第二名「{other}」{views} 次浏览 · {replies} 次回应")
        }),
    }];
    let filed = cited(
        db,
        "SELECT COUNT(*) FROM staff_leads WHERE created_at >= ?1",
        &[&(now_ms() - TOPIC_WINDOW_DAYS * 86_400_000)],
    )?;
    evidence.push(Evidence {
        employee_name: None,
        employee_id: None,
        run_id: None,
        step: None,
        label: format!("窗口内落地 {filed} 张卡片"),
        quote: None,
    });
    propose(
        db,
        &json!({
            "board": "report",
            "title": format!("「{top}」正在起量，下一轮主攻它"),
            "body": format!(
                "最近 {TOPIC_WINDOW_DAYS} 天，「{top}」拿到 {top_views} 次浏览、{top_replies} 次回应，是第二名的 {:.1} 倍。浏览数只说明它被推给了更多人，回应数才说明有人愿意说话。",
                ratio
            ),
            "action": "让笔杆子围绕这个题目多写两篇，地推员优先投放到问这个问题的位置",
            "directive": plan_for(
                db,
                Some(WRITER),
                "围绕这个题目再写两篇，每篇交两版备选，写完放进内容工厂等定稿",
            ),
            // Each number carries its own name: this line once printed the
            // lead count under the word for orders, which is a claim about
            // money that the rows did not support.
            "impact": format!("{} 次浏览、{} 次回应、{} 次咨询、{} 单", top_views, top_replies, top_inquiries, top_deals),
            "costNote": "占用笔杆子一次写作（当前 1 次/天）",
            "evidence": evidence,
        }),
    )
}

/// Replies the boards never turned into cards.
fn unfiled_replies(db: &Database) -> Result<Option<Suggestion>> {
    let today = local_day(now_ms());
    let since = local_day(now_ms() - (TOPIC_WINDOW_DAYS - 1) * 86_400_000);
    let heard = cited(
        db,
        "SELECT COALESCE(SUM(replies), 0) FROM staff_metrics WHERE day >= ?1",
        &[&since],
    )?;
    let filed = cited(
        db,
        "SELECT COUNT(*) FROM staff_leads WHERE created_at >= ?1",
        &[&(now_ms() - TOPIC_WINDOW_DAYS * 86_400_000)],
    )?;
    let gap = heard - filed;
    if heard == 0 || gap < 3 {
        return Ok(None);
    }
    propose(
        db,
        &json!({
            "board": "report",
            "title": format!("{gap} 条回应没有变成卡片"),
            "body": format!("窗口内记到 {heard} 次回应，但只落地了 {filed} 张卡片。回应对不上卡片，说明有人说了话而没人记下来——这是分诊漏了，不是没人感兴趣。"),
            "action": "让接线员按分诊标准重跑一遍最近的回应",
            "directive": plan_for(
                db,
                Some(RECEPTIONIST),
                "按分诊标准重跑一遍最近的回应，把漏掉的补成卡片",
            ),
            "impact": format!("可能捞回 {gap} 条被漏掉的对话"),
            "costNote": "占用接线员一次运行（当前 3 次/天）",
            "evidence": vec![
                json!({
                    "label": format!("窗口内 {heard} 次回应 / {filed} 张卡片"),
                    "quote": format!("统计区间 {since} 至 {today}"),
                }),
            ],
        }),
    )
}

/// The shared wording that has gone stale.
fn stale_playbook(db: &Database) -> Result<Option<Suggestion>> {
    let cutoff = now_ms() - STALE_PLAYBOOK_DAYS * 86_400_000;
    let mut stmt = db.conn().prepare(
        "SELECT memory_key, content, created_at, employee_id, run_id, id
           FROM memory_items
          WHERE scope = 'shared' AND kind = 'playbook' AND status = 'active' AND created_at <= ?1
          ORDER BY created_at
          LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![cutoff, EVIDENCE_LIMIT as i64], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;
    let mut evidence = Vec::new();
    for row in rows {
        let (key, content, created_at, employee_id, run_id) = row?;
        let days = (now_ms() - created_at) / 86_400_000;
        evidence.push(Evidence {
            employee_name: filer(db, employee_id.as_deref()),
            employee_id,
            run_id,
            step: None,
            label: format!("{} · 已 {days} 天", if key.is_empty() { "话术" } else { &key }),
            quote: Some(content.chars().take(240).collect()),
        });
    }
    if evidence.is_empty() {
        return Ok(None);
    }
    let count = evidence.len();
    propose(
        db,
        &json!({
            "board": "arsenal",
            "title": format!("{count} 条话术超过 {STALE_PLAYBOOK_DAYS} 天没更新"),
            "body": "话术里写的卖点会过期：对方看到的那一版，可能还在讲我们上个月的说法。全员每轮开工前都读这里，所以改这里等于全员换口径。",
            "action": "逐条读一遍，过期下架，仍然成立的升为主话术",
            "directive": plan_for(
                db,
                None,
                "逐条读一遍，过期下架，仍然成立的升为主话术",
            ),
            "impact": format!("{count} 条口径影响所有员工的下一轮"),
            "costNote": "占用你 10 分钟，不额外花钱",
            "evidence": evidence,
        }),
    )
}

/// Everything the boards currently have to say, newest reasoning included.
pub fn derive_suggestions(db: &Database, _input: &Value) -> Result<Vec<Suggestion>> {
    let mut raised = Vec::new();
    for candidate in [
        cold_leads(db)?,
        unanswered_replies(db)?,
        drafts_waiting(db)?,
        leading_topic(db)?,
        unfiled_replies(db)?,
        stale_playbook(db)?,
    ]
    .into_iter()
    .flatten()
    {
        raised.push(candidate);
    }
    Ok(raised)
}
