/**
 * Gestión de informes.
 *
 * La lista de lo que el sistema exige entregar, y desde dónde se crea, se activa
 * y se desactiva. Un informe DESACTIVADO no genera entregas nuevas —dejó de ser
 * obligación— pero no se borra: su historia de entregas sigue ahí. Borrar
 * perdería el rastro de lo que sí se exigió mientras estuvo vigente.
 *
 * Crear un informe con su regla se hace en el Cronograma, donde las fechas se ven
 * antes de guardar. Aquí se gobierna lo que ya existe.
 */
import { Component, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router, RouterLink } from '@angular/router';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { MatChipsModule } from '@angular/material/chips';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { MatSlideToggleModule } from '@angular/material/slide-toggle';
import { MatTableModule } from '@angular/material/table';
import { MatTooltipModule } from '@angular/material/tooltip';
import { firstValueFrom } from 'rxjs';

import { Api, InformeFila, ReglaJson } from '../nucleo/api';

/** Del vocabulario del motor al del usuario. La lista se lee, no se descifra. */
const NOMBRE_MODO: Record<string, string> = {
  CICLICO: 'Cíclica',
  ESPECIFICO: 'Periódica',
  SEMANAL: 'Semanal',
  HOURLY: 'Por horas',
  HORARIO: 'Por horas',
  FECHAS: 'Fechas fijas',
  INMEDIATO: 'A solicitud',
};

@Component({
  selector: 'siger-informes',
  imports: [
    FormsModule,
    RouterLink,
    MatButtonModule,
    MatCardModule,
    MatChipsModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatProgressBarModule,
    MatSlideToggleModule,
    MatTableModule,
    MatTooltipModule,
  ],
  template: `
    <header class="cabecera">
      <h2>Informes</h2>
      <span class="empuje"></span>
      <mat-form-field appearance="outline" subscriptSizing="dynamic">
        <mat-label>Filtrar</mat-label>
        <input matInput [(ngModel)]="filtro" (ngModelChange)="texto.set($event)"
               placeholder="nombre o periodicidad" />
        <mat-icon matSuffix>search</mat-icon>
      </mat-form-field>
      <a mat-flat-button routerLink="/cronograma">
        <mat-icon>add</mat-icon> Nuevo informe
      </a>
    </header>

    @if (cargando()) { <mat-progress-bar mode="indeterminate" /> }
    @if (error()) {
      <p class="error"><mat-icon>error_outline</mat-icon> {{ error() }}</p>
    }

    @if (!cargando() && visibles().length === 0) {
      <p class="vacio">
        @if (todos().length === 0) {
          Todavía no hay informes. Cree el primero desde
          <a routerLink="/cronograma">el cronograma</a>.
        } @else {
          Ningún informe coincide con «{{ texto() }}».
        }
      </p>
    }

    @if (visibles().length > 0) {
      <table mat-table [dataSource]="visibles()" class="tabla">
        <ng-container matColumnDef="nombre">
          <th mat-header-cell *matHeaderCellDef>Informe</th>
          <td mat-cell *matCellDef="let i">
            <div class="nombre" [class.inactivo]="!i.activo">{{ i.nombre }}</div>
            @if (i.descripcion) { <div class="desc">{{ i.descripcion }}</div> }
          </td>
        </ng-container>

        <ng-container matColumnDef="modo">
          <th mat-header-cell *matHeaderCellDef>Periodicidad</th>
          <td mat-cell *matCellDef="let i">
            <mat-chip disableRipple>{{ nombreModo(i.modo) }}</mat-chip>
          </td>
        </ng-container>

        <ng-container matColumnDef="estado">
          <th mat-header-cell *matHeaderCellDef>Vigente</th>
          <td mat-cell *matCellDef="let i">
            <mat-slide-toggle [checked]="i.activo" [disabled]="ocupada() === i.id"
                              (change)="alternar(i)"
                              [matTooltip]="i.activo ? 'Activo: genera entregas' : 'Inactivo: no genera entregas nuevas'">
            </mat-slide-toggle>
          </td>
        </ng-container>

        <ng-container matColumnDef="acciones">
          <th mat-header-cell *matHeaderCellDef></th>
          <td mat-cell *matCellDef="let i">
            <a mat-stroked-button [routerLink]="['/informes', i.id, 'partes']"
               matTooltip="Quién interviene: responsable, supervisión, destino">
              <mat-icon>groups</mat-icon> Partes
            </a>
          </td>
        </ng-container>

        <tr mat-header-row *matHeaderRowDef="columnas"></tr>
        <tr mat-row *matRowDef="let fila; columns: columnas;"
            [class.recien]="fila.id === nuevo()"></tr>
      </table>
    }
  `,
  styles: `
    .cabecera { display: flex; align-items: center; gap: .75rem; flex-wrap: wrap; }
    .cabecera h2 { margin: 0; }
    .empuje { flex: 1; }
    .tabla { width: 100%; margin-top: 1rem; background: transparent; }
    .nombre.inactivo { color: var(--mat-sys-on-surface-variant); text-decoration: line-through; }
    .desc { color: var(--mat-sys-on-surface-variant); font-size: .85rem; }
    .recien { animation: destacar 2s ease-out; }
    @keyframes destacar {
      from { background: color-mix(in srgb, var(--mat-sys-primary) 22%, transparent); }
      to { background: transparent; }
    }
    .error { display: flex; align-items: center; gap: .4rem; color: var(--mat-sys-error); }
    .vacio { color: var(--mat-sys-on-surface-variant); margin-top: 1rem; }
  `,
})
export class Informes {
  private api = inject(Api);
  private router = inject(Router);

  readonly columnas = ['nombre', 'modo', 'estado', 'acciones'];

  todos = signal<InformeFila[]>([]);
  cargando = signal(true);
  error = signal('');
  texto = signal('');
  filtro = '';
  /** El id que se está guardando: deshabilita solo esa fila, no toda la tabla. */
  ocupada = signal<number | null>(null);
  /** El recién creado, para resaltarlo un instante al volver del editor. */
  nuevo = signal<number | null>(null);

  readonly visibles = computed(() => {
    const q = this.texto().trim().toLowerCase();
    if (!q) return this.todos();
    return this.todos().filter(
      (i) =>
        i.nombre.toLowerCase().includes(q) ||
        this.nombreModo(i.modo).toLowerCase().includes(q),
    );
  });

  constructor() {
    const nuevo = this.router.parseUrl(this.router.url).queryParams['nuevo'];
    if (nuevo) this.nuevo.set(Number(nuevo));
    void this.cargar();
  }

  nombreModo(modo: string | null) {
    return modo ? (NOMBRE_MODO[modo.toUpperCase()] ?? modo) : '—';
  }

  private async cargar() {
    this.cargando.set(true);
    this.error.set('');
    try {
      this.todos.set(await firstValueFrom(this.api.informes()));
    } catch (e) {
      this.error.set((e as Error).message);
    } finally {
      this.cargando.set(false);
    }
  }

  /** Activar o desactivar. Requiere reenviar la regla porque el editor de un
   *  informe es su regla entera; para no perderla ni reconstruirla a ciegas, se
   *  pide el detalle —que la trae cruda— y se reenvía con el `activo` cambiado. */
  async alternar(informe: InformeFila) {
    this.ocupada.set(informe.id);
    this.error.set('');
    try {
      const detalle = await firstValueFrom(this.api.informeDetalle(informe.id));
      const recurrencia = JSON.parse(detalle.recurrencia) as ReglaJson;
      await firstValueFrom(
        this.api.actualizarInforme(informe.id, {
          nombre: detalle.nombre,
          descripcion: detalle.descripcion,
          recurrencia,
          activo: !informe.activo,
        }),
      );
      // Refleja el cambio sin recargar toda la lista.
      this.todos.update((xs) =>
        xs.map((x) => (x.id === informe.id ? { ...x, activo: !x.activo } : x)),
      );
    } catch (e) {
      this.error.set((e as Error).message);
    } finally {
      this.ocupada.set(null);
    }
  }
}
