/**
 * Bandeja de notificaciones internas (#163).
 *
 * Cada quien ve las suyas (el servidor filtra por el actor). Las no leídas van
 * arriba y resaltadas; al abrir una se marca como leída y, si apunta a una
 * entrega, lleva a la lista de entregas.
 */
import { Component, computed, inject, signal } from '@angular/core';
import { Router } from '@angular/router';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { MatIconModule } from '@angular/material/icon';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { DatePipe } from '@angular/common';
import { firstValueFrom } from 'rxjs';

import { Api, Notificacion } from '../nucleo/api';

@Component({
  selector: 'siger-notificaciones',
  imports: [DatePipe, MatButtonModule, MatCardModule, MatIconModule, MatProgressBarModule],
  template: `
    <header class="cabecera">
      <h2>Notificaciones</h2>
      @if (sinLeer() > 0) { <span class="badge">{{ sinLeer() }} sin leer</span> }
    </header>

    @if (cargando()) { <mat-progress-bar mode="indeterminate" /> }
    @if (error()) { <p class="error"><mat-icon>error_outline</mat-icon> {{ error() }}</p> }
    @if (!cargando() && lista().length === 0) {
      <p class="vacio">No tienes notificaciones.</p>
    }

    @for (n of lista(); track n.id) {
      <mat-card class="item" [class.nueva]="!n.leida" (click)="abrir(n)">
        <mat-card-content>
          <div class="fila">
            <mat-icon [class.dim]="n.leida">{{ icono(n.tipo) }}</mat-icon>
            <div class="cuerpo">
              <div class="titulo">{{ n.titulo }}</div>
              <div class="msg">{{ n.mensaje }}</div>
              <div class="cuando">{{ n.momento | date: 'dd MMM yyyy, HH:mm' }}</div>
            </div>
            @if (!n.leida) { <span class="punto" aria-label="sin leer"></span> }
          </div>
        </mat-card-content>
      </mat-card>
    }
  `,
  styles: `
    .cabecera { display: flex; align-items: center; gap: .75rem; }
    .cabecera h2 { margin: 0; }
    .badge { background: var(--mat-sys-primary); color: var(--mat-sys-on-primary);
      border-radius: 999px; padding: .1rem .6rem; font-size: .8rem; font-weight: 700; }
    .item { margin-top: .6rem; cursor: pointer; }
    .item.nueva { border-left: 3px solid var(--mat-sys-primary); }
    .fila { display: flex; align-items: flex-start; gap: .75rem; }
    .fila mat-icon.dim { color: var(--mat-sys-outline); }
    .cuerpo { flex: 1; min-width: 0; }
    .titulo { font-weight: 650; }
    .msg { color: var(--mat-sys-on-surface-variant); font-size: .9rem; margin: .1rem 0; }
    .cuando { color: var(--mat-sys-outline); font-size: .78rem; }
    .punto { width: .6rem; height: .6rem; border-radius: 999px; background: var(--mat-sys-primary); margin-top: .4rem; }
    .vacio { color: var(--mat-sys-on-surface-variant); margin-top: 1rem; }
    .error { display: flex; align-items: center; gap: .4rem; color: var(--mat-sys-error); }
  `,
})
export class Notificaciones {
  private api = inject(Api);
  private router = inject(Router);
  lista = signal<Notificacion[]>([]);
  cargando = signal(true);
  error = signal('');
  readonly sinLeer = computed(() => this.lista().filter((n) => !n.leida).length);

  constructor() {
    void this.cargar();
  }

  icono(tipo: string) {
    return tipo === 'revision' ? 'rate_review' : tipo === 'justificacion' ? 'gavel' : 'notifications';
  }

  private async cargar() {
    this.cargando.set(true);
    this.error.set('');
    try {
      this.lista.set(await firstValueFrom(this.api.notificaciones()));
    } catch (e) {
      this.error.set((e as Error).message);
    } finally {
      this.cargando.set(false);
    }
  }

  async abrir(n: Notificacion) {
    if (!n.leida) {
      try {
        await firstValueFrom(this.api.marcarLeida(n.id));
        this.lista.update((xs) => xs.map((x) => (x.id === n.id ? { ...x, leida: true } : x)));
      } catch {
        /* marcar leída no es crítico: si falla, la notificación sigue ahí */
      }
    }
    if (n.entrega_id) void this.router.navigate(['/entregas']);
  }
}
