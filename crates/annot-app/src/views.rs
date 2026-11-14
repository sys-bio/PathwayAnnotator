//! The user interface.

use std::rc::Rc;
use std::time::Duration;

use annot_engine::{Confidence, Role};
use annot_model::Element;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::browser;
use crate::session::{Counts, Session, Status};

type SessionSignal = RwSignal<Option<Session>, LocalStorage>;

/// The example model offered on the start screen.
const EXAMPLE: &str = include_str!("../assets/glycolysis-example.ant");

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Molecules,
    Reactions,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    Review,
    Done,
    Nothing,
}

impl Filter {
    fn keeps(self, s: &Status) -> bool {
        match self {
            Filter::All => true,
            Filter::Review => matches!(s, Status::Suggested { .. }),
            Filter::Done => matches!(s, Status::Identified { .. } | Status::AnnotatedElsewhere),
            Filter::Nothing => matches!(s, Status::Unknown),
        }
    }
}

/// Shared by every component.
#[derive(Clone, Copy)]
struct Ctx {
    session: SessionSignal,
    message: RwSignal<Option<String>>,
    tab: RwSignal<Tab>,
    filter: RwSignal<Filter>,
    selected: RwSignal<Option<Element>>,
}

fn dom_id(el: Element) -> String {
    match el {
        Element::Species(i) => format!("species-{i}"),
        Element::Reaction(i) => format!("reaction-{i}"),
        Element::Compartment(i) => format!("compartment-{i}"),
        Element::Model => "model".into(),
    }
}

fn level_text(level: Confidence) -> &'static str {
    match level {
        Confidence::High => "high confidence",
        Confidence::Likely => "likely",
        Confidence::Uncertain => "uncertain",
    }
}

/// A suggestion's name as a link to its database entry (new tab), so people
/// can check it themselves; plain bold text if there is nowhere to go.
/// Clicking it does not open or close the row it is in.
fn check_link(label: String, url: Option<String>) -> AnyView {
    match url {
        Some(url) => view! {
            <a
                class="check"
                href=url
                target="_blank"
                rel="noopener noreferrer"
                title="Check this in its database (opens a new tab)"
                on:click=|ev: web_sys::MouseEvent| ev.stop_propagation()
            >
                {label}
                <svg class="ext" viewBox="0 0 12 12" width="11" height="11" aria-hidden="true">
                    <path d="M5 2H2v8h8V7M7 1h4v4M11 1L5.5 6.5" fill="none" stroke="currentColor" stroke-width="1.3"/>
                </svg>
            </a>
        }
            .into_any(),
        None => view! { <b>{label}</b> }.into_any(),
    }
}

#[component]
pub fn App() -> impl IntoView {
    let pack: RwSignal<Option<Rc<kpack::Pack>>, LocalStorage> = RwSignal::new_local(None);
    let pack_error = RwSignal::new(None::<String>);
    let ctx = Ctx {
        session: RwSignal::new_local(None),
        message: RwSignal::new(None),
        tab: RwSignal::new(Tab::Molecules),
        filter: RwSignal::new(Filter::All),
        selected: RwSignal::new(None),
    };
    provide_context(ctx);

    spawn_local(async move {
        match browser::fetch_pack("kpack.bin").await {
            Ok(p) => pack.set(Some(Rc::new(p))),
            Err(e) => pack_error.set(Some(e)),
        }
    });

    let open = move |text: String, name: String| {
        let Some(p) = pack.get_untracked() else {
            ctx.message.set(Some("The knowledge base is still loading; try again in a moment.".into()));
            return;
        };
        match annot_model::load(&text) {
            Ok(ws) => {
                ctx.session.set(Some(Session::new(p, ws, name)));
                ctx.selected.set(None);
                ctx.filter.set(Filter::All);
                ctx.message.set(None);
            }
            Err(e) => ctx.message.set(Some(format!("Could not open {name}: {e}."))),
        }
    };

    let pack_ready = Memo::new(move |_| pack.with(|p| p.is_some()));
    let has_session = Memo::new(move |_| ctx.session.with(|s| s.is_some()));

    view! {
        <Header/>
        <main>
            {move || {
                if let Some(e) = pack_error.get() {
                    view! {
                        <p class="error">
                            "The knowledge base could not be loaded (" {e} "). Reload the page to try again."
                        </p>
                    }
                        .into_any()
                } else if !pack_ready.get() {
                    view! { <p class="loading">"Loading the knowledge base..."</p> }.into_any()
                } else if !has_session.get() {
                    view! { <Start open=open/> }.into_any()
                } else {
                    view! {
                        <Overview/>
                        <Tabs/>
                    }
                        .into_any()
                }
            }}
            <Message/>
        </main>
    }
}

#[component]
fn Header() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let s = ctx.session;
    let file = move || s.with(|s| s.as_ref().map(|s| s.file_name.clone()));
    let can_undo = move || s.with(|s| s.as_ref().is_some_and(|s| s.ws.can_undo()));
    let can_redo = move || s.with(|s| s.as_ref().is_some_and(|s| s.ws.can_redo()));
    let save = move |_| {
        let out = s.with_untracked(|s| s.as_ref().map(|s| (s.export_name(), s.ws.to_sbml())));
        if let Some((name, text)) = out {
            match browser::save_file(&name, &text) {
                Ok(()) => ctx.message.set(Some(format!("Saved {name}."))),
                Err(e) => ctx.message.set(Some(format!("Could not save: {e}."))),
            }
        }
    };
    view! {
        <header class="top">
            <h1>"PathwayAnnotator"</h1>
            <span class="file">{file}</span>
            <Show when=move || file().is_some()>
                <div class="actions">
                    <button on:click=move |_| s.update(|s| if let Some(s) = s { s.undo() }) disabled=move || !can_undo()>
                        "Undo"
                    </button>
                    <button on:click=move |_| s.update(|s| if let Some(s) = s { s.redo() }) disabled=move || !can_redo()>
                        "Redo"
                    </button>
                    <button class="primary" on:click=save>"Save annotated SBML"</button>
                    <button on:click=move |_| s.set(None)>"Close"</button>
                </div>
            </Show>
        </header>
    }
}

#[component]
fn Start<F>(open: F) -> impl IntoView
where
    F: Fn(String, String) + Copy + Send + Sync + 'static,
{
    let ctx = expect_context::<Ctx>();
    let dragging = RwSignal::new(false);
    let read = move |file: web_sys::File| {
        spawn_local(async move {
            match browser::read_file(&file).await {
                Ok(text) => open(text, file.name()),
                Err(e) => ctx.message.set(Some(format!("Could not read {}: {e}.", file.name()))),
            }
        });
    };
    view! {
        <section class="start">
            <h2>"Annotate a model"</h2>
            <p>
                "Open an SBML or Antimony model. The annotator suggests what each molecule and reaction is, "
                "with the evidence, for you to confirm. Your model stays on this computer."
            </p>
            <label
                class="drop"
                class:dragging=move || dragging.get()
                on:dragover=move |ev: web_sys::DragEvent| {
                    ev.prevent_default();
                    dragging.set(true);
                }
                on:dragleave=move |_| dragging.set(false)
                on:drop=move |ev: web_sys::DragEvent| {
                    ev.prevent_default();
                    dragging.set(false);
                    if let Some(file) = ev.data_transfer().and_then(|d| d.files()).and_then(|f| f.get(0)) {
                        read(file);
                    }
                }
            >
                <input
                    type="file"
                    accept=".xml,.sbml,.ant,.txt"
                    on:change=move |ev| {
                        let input: web_sys::HtmlInputElement = event_target(&ev);
                        if let Some(file) = input.files().and_then(|f| f.get(0)) {
                            read(file);
                        }
                    }
                />
                <span class="drop-text">"Drop a model file here, or click to choose one"</span>
            </label>
            <p class="or">
                "No model at hand? "
                <button class="link" on:click=move |_| open(EXAMPLE.to_string(), "glycolysis-example.ant".into())>
                    "Try the glycolysis example"
                </button>
            </p>
        </section>
    }
}

#[component]
fn Overview() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let s = ctx.session;
    let counts = move || {
        s.with(|s| {
            s.as_ref()
                .map(|s| (s.counts(&s.species()), s.counts(&s.reactions())))
                .unwrap_or_default()
        })
    };
    let high = move || {
        let (a, b) = counts();
        a.high + b.high
    };
    let accept_high = move |_| {
        let mut n = 0;
        s.update(|s| {
            if let Some(s) = s {
                n = s.accept_all_high();
            }
        });
        ctx.message.set(Some(format!("Accepted {n} high-confidence suggestions. Undo reverts them one at a time.")));
    };
    view! {
        <section class="overview">
            <Progress title="Molecules" counts=Signal::derive(move || counts().0)/>
            <Progress title="Reactions" counts=Signal::derive(move || counts().1)/>
            <div class="bulk">
                <button class="primary" disabled=move || high() == 0 on:click=accept_high>
                    {move || format!("Accept all high-confidence suggestions ({})", high())}
                </button>
            </div>
        </section>
    }
}

#[component]
fn Progress(title: &'static str, counts: Signal<Counts>) -> impl IntoView {
    let pct = move |n: usize| {
        let c = counts.get();
        if c.total == 0 { 0.0 } else { 100.0 * n as f64 / c.total as f64 }
    };
    view! {
        <div class="progress">
            <div class="progress-head">
                <b>{title}</b>
                <span>{move || {
                    let c = counts.get();
                    format!("{} of {} identified", c.done, c.total)
                }}</span>
            </div>
            <div class="bar" role="img" aria-label=move || {
                let c = counts.get();
                format!("{} identified, {} high, {} likely, {} uncertain, {} with no suggestion", c.done, c.high, c.likely, c.uncertain, c.unknown)
            }>
                <span class="seg done" style=move || format!("width:{}%", pct(counts.get().done))></span>
                <span class="seg high" style=move || format!("width:{}%", pct(counts.get().high))></span>
                <span class="seg likely" style=move || format!("width:{}%", pct(counts.get().likely))></span>
                <span class="seg uncertain" style=move || format!("width:{}%", pct(counts.get().uncertain))></span>
            </div>
            <div class="legend">
                <span><i class="dot done"></i>{move || format!("identified {}", counts.get().done)}</span>
                <span><i class="dot high"></i>{move || format!("high {}", counts.get().high)}</span>
                <span><i class="dot likely"></i>{move || format!("likely {}", counts.get().likely)}</span>
                <span><i class="dot uncertain"></i>{move || format!("uncertain {}", counts.get().uncertain)}</span>
                <span><i class="dot unknown"></i>{move || format!("no suggestion {}", counts.get().unknown)}</span>
            </div>
        </div>
    }
}

#[component]
fn Tabs() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let tab_button = move |t: Tab, label: &'static str| {
        view! {
            <button
                role="tab"
                class="tab"
                class:active=move || ctx.tab.get() == t
                aria-selected=move || (ctx.tab.get() == t).to_string()
                on:click=move |_| ctx.tab.set(t)
            >
                {label}
            </button>
        }
    };
    let filter_button = move |f: Filter, label: &'static str| {
        view! {
            <button class="chip-filter" class:active=move || ctx.filter.get() == f on:click=move |_| ctx.filter.set(f)>
                {label}
            </button>
        }
    };
    view! {
        <nav class="tabs" role="tablist">
            {tab_button(Tab::Molecules, "Molecules")}
            {tab_button(Tab::Reactions, "Reactions")}
            <span class="filters">
                {filter_button(Filter::All, "All")}
                {filter_button(Filter::Review, "To review")}
                {filter_button(Filter::Done, "Identified")}
                {filter_button(Filter::Nothing, "No suggestion")}
            </span>
        </nav>
        <ItemList/>
    }
}

#[component]
fn ItemList() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let items = move || {
        let tab = ctx.tab.get();
        let filter = ctx.filter.get();
        ctx.session.with(|s| {
            let Some(s) = s else { return Vec::new() };
            let all = if tab == Tab::Molecules { s.species() } else { s.reactions() };
            all.into_iter().filter(|el| filter.keeps(&s.status(*el))).collect::<Vec<_>>()
        })
    };
    view! {
        <ul class="items">
            <For each=items key=|el| *el children=move |el| view! { <Item el=el/> }/>
        </ul>
        <Show when=move || items().is_empty()>
            <p class="empty">"Nothing here with this filter."</p>
        </Show>
    }
}

#[component]
fn Item(el: Element) -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let s = ctx.session;
    let name = move || s.with(|s| s.as_ref().map(|s| s.display_name(el)).unwrap_or_default());
    let id = move || {
        s.with(|s| {
            s.as_ref()
                .map(|s| s.ws.element_id(el).to_string())
                .filter(|id| *id != name())
                .unwrap_or_default()
        })
    };
    let status = move || s.with(|s| s.as_ref().map(|s| s.status(el)).unwrap_or(Status::Unknown));
    let open = move || ctx.selected.get() == Some(el);
    let toggle = move |_| ctx.selected.update(|sel| *sel = if *sel == Some(el) { None } else { Some(el) });
    let accept_first = move |ev: web_sys::MouseEvent| {
        ev.stop_propagation();
        s.update(|s| {
            if let Some(s) = s
                && let Some(c) = s.candidates(el).first()
            {
                let msg = s.accept(el, c.target);
                ctx.message.set(Some(msg));
            }
        });
    };
    // Where to check the suggestion, or the current identification.
    let suggestion_link = move || {
        s.with(|s| {
            s.as_ref()
                .and_then(|s| s.candidates(el).first().and_then(|c| s.main_link(c.target)))
        })
    };
    let identity_link = move || s.with(|s| s.as_ref().and_then(|s| s.ws.identity_uris(el).into_iter().next()));
    let what = move || match status() {
        Status::Identified { label } => view! {
            <span class="what">"Identified as " {check_link(label, identity_link())}</span>
        }
            .into_any(),
        Status::AnnotatedElsewhere => {
            view! { <span class="what">"Annotated (with something outside our database)"</span> }.into_any()
        }
        Status::Suggested { label, level } => view! {
            <span class="what">
                {check_link(label, suggestion_link())}
                <span class=format!("badge {}", Status::Suggested { label: String::new(), level }.class())>
                    {level_text(level)}
                </span>
            </span>
            <button class="accept" on:click=accept_first>"Accept"</button>
        }
            .into_any(),
        Status::Unknown => view! { <span class="what muted">"No suggestion"</span> }.into_any(),
    };
    view! {
        <li class="item" class:open=open id=dom_id(el)>
            <div class="row" on:click=toggle>
                <span class=move || format!("dot {}", status().class())></span>
                <span class="name">{name}</span>
                <span class="id">{id}</span>
                {what}
                // An explicit control: the whole row opens too, but nothing
                // else says so.
                <button
                    class="disclose"
                    aria-expanded=move || open().to_string()
                    title="Show the suggestions, their evidence and the current annotation"
                    on:click=move |ev: web_sys::MouseEvent| {
                        ev.stop_propagation();
                        ctx.selected.update(|sel| *sel = if *sel == Some(el) { None } else { Some(el) });
                    }
                >
                    {move || if open() { "Hide" } else { "Details" }}
                    <svg class="chevron" viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
                        <path d="M3 4.5l3 3 3-3" fill="none" stroke="currentColor" stroke-width="1.5"/>
                    </svg>
                </button>
            </div>
            {matches!(el, Element::Reaction(_)).then(|| view! { <Equation el=el/> })}
            <Show when=open>
                <Details el=el/>
            </Show>
        </li>
    }
}

/// A reaction as "A + B -> C", each molecule a chip coloured by its status.
/// Clicking a chip shows that molecule.
#[component]
fn Equation(el: Element) -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let Element::Reaction(ri) = el else { return ().into_any() };
    let side = move |role: Role| {
        ctx.session.with(|s| {
            let Some(s) = s else { return Vec::new() };
            s.model.reactions[ri]
                .participants
                .iter()
                .filter(|p| p.role == role)
                .map(|p| {
                    let sp = Element::Species(p.species);
                    let n = if p.stoichiometry == 1.0 { String::new() } else { format!("{} ", p.stoichiometry) };
                    (sp, format!("{n}{}", s.display_name(sp)), s.status(sp).class())
                })
                .collect::<Vec<_>>()
        })
    };
    let chip = move |(sp, label, class): (Element, String, &'static str)| {
        view! {
            <button
                class=format!("chip {class}")
                title="Show this molecule"
                on:click=move |ev: web_sys::MouseEvent| {
                    ev.stop_propagation();
                    ctx.tab.set(Tab::Molecules);
                    ctx.filter.set(Filter::All);
                    ctx.selected.set(Some(sp));
                    set_timeout(
                        move || {
                            if let Some(e) = document().get_element_by_id(&dom_id(sp)) {
                                e.scroll_into_view();
                            }
                        },
                        Duration::from_millis(50),
                    );
                }
            >
                {label}
            </button>
        }
    };
    let join = move |items: Vec<(Element, String, &'static str)>| {
        let n = items.len();
        items
            .into_iter()
            .enumerate()
            .map(|(i, it)| view! { {chip(it)} {(i + 1 < n).then_some(" + ")} })
            .collect_view()
    };
    view! {
        <div class="equation">
            {move || join(side(Role::Reactant))}
            <span class="arrow">" -> "</span>
            {move || join(side(Role::Product))}
            {move || {
                let m = side(Role::Modifier);
                (!m.is_empty()).then(|| view! { <span class="modifiers">"  (modified by " {join(m)} ")"</span> })
            }}
        </div>
    }
        .into_any()
}

/// The candidates for an element, with their evidence, and its existing
/// annotation.
#[component]
fn Details(el: Element) -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let s = ctx.session;
    let rows = move || {
        s.with(|s| {
            let Some(s) = s else { return Vec::new() };
            s.candidates(el)
                .iter()
                .take(5)
                .map(|c| {
                    (
                        c.target,
                        s.label(c.target),
                        s.links(c.target),
                        c.evidence.iter().map(|e| e.note.clone()).collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>()
        })
    };
    let existing = move || s.with(|s| s.as_ref().map(|s| s.ws.terms(el)).unwrap_or_default());
    view! {
        <div class="details">
            <Show when=move || !existing().is_empty()>
                <h4>"Current annotation"</h4>
                <ul class="terms">
                    {move || {
                        existing()
                            .into_iter()
                            .map(|t| {
                                let q = if t.model { format!("model {}", t.qualifier) } else { t.qualifier.clone() };
                                view! {
                                    <li>
                                        <span class="qualifier">{q}</span>
                                        " "
                                        <a href=t.uri.clone() target="_blank" rel="noopener">{t.uri.clone()}</a>
                                    </li>
                                }
                            })
                            .collect_view()
                    }}
                </ul>
            </Show>
            <h4>"Suggestions"</h4>
            {move || {
                let rows = rows();
                if rows.is_empty() {
                    return view! { <p class="muted">"The annotator has nothing to suggest for this one."</p> }.into_any();
                }
                rows.into_iter()
                    .enumerate()
                    .map(|(i, (target, label, links, notes))| {
                        let main = links.first().map(|(_, url)| url.clone());
                        view! {
                            <div class="cand">
                                <div class="cand-head">
                                    <span class="rank">{i + 1}</span>
                                    {check_link(label, main)}
                                    <span class="ids">
                                        {links
                                            .into_iter()
                                            .map(|(text, url)| {
                                                view! {
                                                    <a href=url target="_blank" rel="noopener noreferrer">
                                                        {text}
                                                    </a>
                                                }
                                            })
                                            .collect_view()}
                                    </span>
                                    <button
                                        class="accept"
                                        on:click=move |_| {
                                            s.update(|s| {
                                                if let Some(s) = s {
                                                    let msg = s.accept(el, target);
                                                    ctx.message.set(Some(msg));
                                                }
                                            })
                                        }
                                    >
                                        "Accept"
                                    </button>
                                </div>
                                <ul class="evidence">
                                    {notes.into_iter().map(|n| view! { <li>{n}</li> }).collect_view()}
                                </ul>
                            </div>
                        }
                    })
                    .collect_view()
                    .into_any()
            }}
        </div>
    }
}

#[component]
fn Message() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    view! {
        <Show when=move || ctx.message.with(|m| m.is_some())>
            <div class="message" role="status">
                <span>{move || ctx.message.get().unwrap_or_default()}</span>
                <button class="link" on:click=move |_| ctx.message.set(None)>"Dismiss"</button>
            </div>
        </Show>
    }
}
