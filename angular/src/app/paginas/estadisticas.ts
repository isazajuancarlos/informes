/**
 * Estadísticas de cumplimiento.
 *
 * Los números llegan YA recortados por el alcance del actor (el servidor decide,
 * no un parámetro del cliente): un operador ve los suyos; supervisión y admin,
 * todos; un tercero, los de su stakeholder. La misma página sirve a todos los
 * roles. Se muestra primero lo que exige atención —tasa, vencidas, próximas— y
 * luego el detalle, que es como se lee un tablero: se escanea, no se recorre.
 */
import { Component, computed, inject, signal } from '@angular/core';
import { MatCardModule } from '@angular/material/card';
import { MatIconModule } from '@angular/material/icon';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { firstValueFrom } from 'rxjs';

import { Api, Estadisticas } from '../nucleo/api';

@Component({
  selector: 'siger-estadisticas',
  imports: [MatCardModule, MatIconModule, MatProgressBarModule],
  template: `
    <h2>Estadísticas</h2>
    <p class="guia">Cumplimiento según tu alcance. La tasa se mide sobre lo que ya venció y
      debía estar entregado — lo que aún no vence no cuenta.</p>

    @if (cargando()) { <mat-progress-bar mode="indeterminate" /> }
    @if (error()) { <p class="error"><mat-icon>error_outline</mat-icon> {{ error() }}</p> }

    @if (e(); as s) {
      <!-- Lo que exige atención, arriba -->
      <div class="destacado">
        <mat-card class="tasa" [class]="'grado ' + gradoTasa()">
          <mat-card-content>
            <div class="cap">Tasa de cumplimiento</div>
            <div class="big">{{ s.tasa_cumplimiento === null ? '—' : s.tasa_cumplimiento + '%' }}</div>
            <div class="sub">{{ s.tasa_cumplimiento === null ? 'nada ha vencido aún' : 'de lo ya vencido' }}</div>
          </mat-card-content>
        </mat-card>

        <mat-card class="alerta" [class.vacia]="s.vencidas === 0">
          <mat-card-content>
            <mat-icon>report_problem</mat-icon>
            <div><div class="big">{{ s.vencidas }}</div><div class="sub">vencidas sin registrar</div></div>
          </mat-card-content>
        </mat-card>

        <mat-card class="proxima" [class.vacia]="s.proximas_7d === 0">
          <mat-card-content>
            <mat-icon>schedule</mat-icon>
            <div><div class="big">{{ s.proximas_7d }}</div><div class="sub">vencen en 7 días</div></div>
          </mat-card-content>
        </mat-card>
      </div>

      <!-- Desglose por estado -->
      <div class="rejilla">
        @for (c of tarjetas(); track c.clave) {
          <mat-card [class]="'estado ' + c.clave">
            <mat-card-content>
              <div class="n">{{ c.n }}</div>
              <div class="et">{{ c.etiqueta }}</div>
            </mat-card-content>
          </mat-card>
        }
      </div>
      <p class="total">{{ s.total }} entrega(s) en total dentro de tu alcance.</p>
    }
  `,
  styles: `
    .guia { color: var(--mat-sys-on-surface-variant); margin-top: -.5rem; max-width: 60ch; }
    .destacado { display: grid; grid-template-columns: repeat(auto-fit, minmax(13rem, 1fr)); gap: 1rem; margin-top: 1rem; }
    .tasa .big { font-size: 2.6rem; font-weight: 800; line-height: 1; font-variant-numeric: tabular-nums; }
    .cap { font-size: .72rem; letter-spacing: .12em; text-transform: uppercase; color: var(--mat-sys-on-surface-variant); }
    .sub { color: var(--mat-sys-on-surface-variant); font-size: .82rem; margin-top: .25rem; }
    .tasa.grado-bien { border-left: 4px solid #2e7d32; }
    .tasa.grado-medio { border-left: 4px solid #f9a825; }
    .tasa.grado-mal { border-left: 4px solid #c62828; }
    .tasa.grado-na { border-left: 4px solid var(--mat-sys-outline); }
    .alerta mat-card-content, .proxima mat-card-content { display: flex; align-items: center; gap: .75rem; }
    .alerta .big, .proxima .big { font-size: 1.9rem; font-weight: 800; line-height: 1; font-variant-numeric: tabular-nums; }
    .alerta mat-icon { color: #c62828; } .proxima mat-icon { color: var(--mat-sys-primary); }
    .alerta.vacia mat-icon, .proxima.vacia mat-icon { color: var(--mat-sys-outline); }
    .rejilla { display: grid; grid-template-columns: repeat(auto-fit, minmax(9rem, 1fr)); gap: .75rem; margin-top: 1.25rem; }
    .estado .n { font-size: 1.8rem; font-weight: 800; font-variant-numeric: tabular-nums; }
    .estado .et { color: var(--mat-sys-on-surface-variant); font-size: .85rem; }
    .estado.cumplidas { border-top: 3px solid #2e7d32; }
    .estado.extemporaneas { border-top: 3px solid #f9a825; }
    .estado.incumplidas { border-top: 3px solid #c62828; }
    .estado.pendientes { border-top: 3px solid var(--mat-sys-outline); }
    .total { color: var(--mat-sys-on-surface-variant); margin-top: 1rem; }
    .error { display: flex; align-items: center; gap: .4rem; color: var(--mat-sys-error); }
  `,
})
export class EstadisticasPagina {
  private api = inject(Api);
  e = signal<Estadisticas | null>(null);
  cargando = signal(true);
  error = signal('');

  readonly gradoTasa = computed(() => {
    const t = this.e()?.tasa_cumplimiento;
    if (t === null || t === undefined) return 'grado-na';
    if (t >= 90) return 'grado-bien';
    if (t >= 70) return 'grado-medio';
    return 'grado-mal';
  });

  readonly tarjetas = computed(() => {
    const s = this.e();
    if (!s) return [];
    return [
      { clave: 'pendientes', etiqueta: 'Pendientes', n: s.pendientes },
      { clave: 'cumplidas', etiqueta: 'Cumplidas', n: s.cumplidas },
      { clave: 'extemporaneas', etiqueta: 'Extemporáneas', n: s.extemporaneas },
      { clave: 'incumplidas', etiqueta: 'Incumplidas', n: s.incumplidas },
    ];
  });

  constructor() {
    void this.cargar();
  }

  private async cargar() {
    this.cargando.set(true);
    this.error.set('');
    try {
      this.e.set(await firstValueFrom(this.api.estadisticas()));
    } catch (err) {
      this.error.set((err as Error).message);
    } finally {
      this.cargando.set(false);
    }
  }
}
